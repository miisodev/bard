use std::{
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        Arc, Mutex, MutexGuard,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rubato::{FftFixedOut, Resampler};
use tauri::{AppHandle, Emitter};

use crate::ttl::Ttl;

pub const TARGET_RATE: u32 = 16000;
/// One resampled frame. earshot's VAD is specified for exactly 256 samples at
/// 16 kHz, and the VAD's whole timing policy is denominated in these 16 ms
/// frames, so this is derived from the detector rather than chosen here.
const CHUNK_OUT: usize = super::vad::FRAME_SAMPLES;
const SILENCE_AUTO_STOP_MS: u64 = 1600;
/// How often the monitor examines the take. This drives the auto-stop, the
/// stall guard and the stream-error check, none of which may be throttled with
/// the display channel.
const MONITOR_TICK: Duration = Duration::from_millis(50);
/// How often the level meter is allowed to move. It is a display channel: every
/// message re-renders the whole webview, so a 50 ms cadence for a 20-minute
/// take is about 24,000 full panel renders to animate a 32-pixel bar.
const MIC_LEVEL_INTERVAL: Duration = Duration::from_millis(200);
/// Meter resolution. A 212 px pill cannot show more than this, and quantising
/// is what lets an unchanged meter emit nothing at all.
const LEVEL_STEPS: f32 = 32.0;
const PROBABILITY_STEPS: f32 = 64.0;
/// A stream that stops delivering frames must not hold a take open for ever:
/// with no frames there is no silence either, so the auto-stop can never fire
/// and the pill would sit on "listening" until Bard was restarted. Three
/// seconds is two orders of magnitude longer than a callback period even on a
/// badly starved thread, so this cannot fire on a slow machine.
const STALL_LIMIT: Duration = Duration::from_secs(3);
const MAX_RECORDING_MINUTES: usize = 20;
const MAX_RECORDING_SAMPLES: usize = TARGET_RATE as usize * 60 * MAX_RECORDING_MINUTES;
const MAX_INPUT_BUFFER_SAMPLES: usize = 32_768;

static STREAM_ERROR: AtomicBool = AtomicBool::new(false);

/// Values shared with the lightweight UI monitor. They are atomics because the
/// CPAL callback is a real-time/audio thread; it must never emit Tauri events or
/// call into a window directly.
struct MonitorState {
    active: AtomicBool,
    level_bits: AtomicU32,
    probability_bits: AtomicU32,
    silence_ms: AtomicU64,
}

impl MonitorState {
    fn new() -> Self {
        Self {
            active: AtomicBool::new(true),
            level_bits: AtomicU32::new(0f32.to_bits()),
            probability_bits: AtomicU32::new(0f32.to_bits()),
            silence_ms: AtomicU64::new(0),
        }
    }

    fn level(&self) -> f32 {
        f32::from_bits(self.level_bits.load(Ordering::Relaxed))
    }

    fn probability(&self) -> f32 {
        f32::from_bits(self.probability_bits.load(Ordering::Relaxed))
    }
}

/// Everything the data callback touches other than the shared state.
///
/// This is built once, in `start`, before the stream exists, and then owned by
/// the callback alone — it is not behind a mutex and no other thread can reach
/// it. The callback used to allocate a `Vec` for the downmixed block, another
/// for the resampler output, and a third for the pending input on *every*
/// invocation, and to construct the resampler — with its FFT plan — on the first
/// frame. A real-time callback that allocates is a callback that can block in
/// the allocator, and that is the textbook cause of an audio glitch.
struct Scratch {
    resampler: FftFixedOut<f32>,
    /// This callback's samples, downmixed to mono, waiting to join `fifo`.
    block: Vec<f32>,
    /// The resampler's pending input at the device rate, preallocated to the
    /// cap so the callback never reallocates it.
    fifo: Vec<f32>,
    /// One resampled frame, preallocated: `process_into_buffer` writes into it.
    frame: Vec<f32>,
}

impl Scratch {
    /// `sample_rate` is the device's rate; the resampler is built here, on a
    /// normal thread, rather than on the first audio callback.
    fn new(sample_rate: u32) -> Result<Self, String> {
        let resampler = FftFixedOut::<f32>::new(
            sample_rate as usize,
            TARGET_RATE as usize,
            CHUNK_OUT,
            1,
            1,
        )
        .map_err(|error| {
            format!(
                "The microphone's {sample_rate} Hz format cannot be converted for dictation: {error}"
            )
        })?;
        Ok(Self {
            resampler,
            block: Vec::new(),
            fifo: Vec::with_capacity(MAX_INPUT_BUFFER_SAMPLES),
            frame: vec![0.0; CHUNK_OUT],
        })
    }
}

struct Shared {
    samples: Mutex<Vec<f32>>,
    vad: Mutex<crate::dictation::vad::VadState>,
    /// Resampled frames produced. The monitor uses this to tell a live stream
    /// from a dead one, which silence timing alone cannot do.
    frames: AtomicU64,
    monitor: Arc<MonitorState>,
    failed: AtomicBool,
    limit_reached: AtomicBool,
}

static RECORDER: Mutex<Option<Recorder>> = Mutex::new(None);

struct Recorder {
    _stream: cpal::Stream,
    shared: Arc<Shared>,
    monitor: Option<JoinHandle<()>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// How long a microphone-presence answer is reused.
///
/// `mic_ready` is a CPAL device enumeration, and `get_state` reads it on every
/// refresh — a panel tick, a capture, a settings save — which made the
/// enumeration show up on the Windows main thread for an answer that only
/// changes when a device is plugged in, unplugged or disabled. Five seconds is
/// below the time in which anyone could notice and care, and above the refresh
/// cadence that made it a cost.
const MIC_READY_TTL: Duration = Duration::from_secs(5);

static MIC_READY: Ttl<bool> = Ttl::new(MIC_READY_TTL);

pub fn mic_ready() -> bool {
    if let Some(ready) = MIC_READY.get() {
        return ready;
    }
    let ready = cpal::default_host().default_input_device().is_some();
    MIC_READY.set(ready);
    ready
}

pub fn list_input_devices() -> Vec<String> {
    let host = cpal::default_host();
    host.input_devices()
        .map(|devs| devs.filter_map(|d| d.name().ok()).collect::<Vec<_>>())
        .unwrap_or_default()
}

fn resolve_device(app: &AppHandle) -> Option<cpal::Device> {
    let wanted = crate::db::get_setting(app, "selected_microphone")?;
    if wanted.is_empty() {
        return None;
    }
    let host = cpal::default_host();
    host.input_devices().ok().and_then(|devs| {
        devs.into_iter()
            .find(|d| d.name().ok().as_deref() == Some(wanted.as_str()))
    })
}

fn level_of(frame: &[f32]) -> f32 {
    if frame.is_empty() {
        return 0.0;
    }
    let rms = (frame.iter().map(|sample| sample * sample).sum::<f32>() / frame.len() as f32).sqrt();
    (rms * 6.0).clamp(0.0, 1.0)
}

fn mark_failed(shared: &Shared) {
    shared.failed.store(true, Ordering::Release);
}

fn append_bounded_samples(samples: &mut Vec<f32>, output: &[f32], limit: usize) -> (usize, bool) {
    let remaining = limit.saturating_sub(samples.len());
    let stored = output.len().min(remaining);
    samples.extend_from_slice(&output[..stored]);
    (stored, stored < output.len())
}

/// Make room for `frames` more samples, starting from empty. The block keeps
/// whatever capacity it grows to, so this allocates at most once per take and
/// never per callback.
fn reserve_block(block: &mut Vec<f32>, frames: usize) {
    block.clear();
    if block.capacity() < frames {
        // `reserve` is relative to `len`, which is zero here, so this is a
        // request for a capacity of at least `frames` and not a subtraction
        // that can underflow when a frame shrinks.
        block.reserve(frames);
    }
}

fn quiet(shared: &Shared) -> bool {
    shared.failed.load(Ordering::Acquire) || shared.limit_reached.load(Ordering::Acquire)
}

/// Take one device callback from the audio thread.
///
/// Only two mutexes are touched, both of them for the length of a memcpy or a
/// VAD frame and neither across a blocking call, and nothing is allocated. The
/// resampler and its FFT plan were built before the stream existed.
fn process_block(
    shared: &Shared,
    resampler: &mut FftFixedOut<f32>,
    fifo: &mut Vec<f32>,
    frame: &mut [f32],
    block: &[f32],
) {
    if quiet(shared) || block.is_empty() {
        return;
    }
    if fifo.len().saturating_add(block.len()) > MAX_INPUT_BUFFER_SAMPLES {
        mark_failed(shared);
        return;
    }
    fifo.extend_from_slice(block);

    let needed = resampler.input_frames_next();
    if fifo.len() < needed {
        return;
    }
    // Exactly the samples the resampler asked for, in stack arrays: no `Vec`,
    // and no ambiguity about how much it will take.
    let source: [&[f32]; 1] = [&fifo[..needed]];
    let mut sink: [&mut [f32]; 1] = [frame];
    if resampler
        .process_into_buffer(&source, &mut sink, None)
        .is_err()
    {
        mark_failed(shared);
        return;
    }
    // Consume what was resampled by shifting the remainder down. `copy_within`
    // is a memmove; nothing is reallocated.
    fifo.copy_within(needed.., 0);
    fifo.truncate(fifo.len() - needed);

    let stored_len = {
        let mut samples = lock(&shared.samples);
        append_bounded_samples(&mut samples, frame, MAX_RECORDING_SAMPLES).0
    };
    if stored_len < frame.len() {
        // The recording limit was reached mid-frame. Only whole frames are ever
        // fed to the VAD: earshot returns -1.0 for a short frame, so a partial
        // one would publish a nonsense probability and reset nothing.
        shared.limit_reached.store(true, Ordering::Release);
    }
    if stored_len == 0 {
        return;
    }
    let stored_output = &frame[..stored_len];

    // The Silero model is real work on the audio thread: a 1024-point FFT and
    // about 18,000 multiply-accumulates per 16 ms frame, which is tens of
    // microseconds. Moving it off the callback needs a queue and a second
    // thread, and the timing it produces is already the user's, so it stays
    // here and the callback's own allocations and FFT plan do not.
    let (probability, silence_ms) = {
        let mut vad = lock(&shared.vad);
        if stored_len == frame.len() {
            let probability = vad.feed(stored_output);
            (probability, vad.silence_ms())
        } else {
            (f32::NAN, vad.silence_ms())
        }
    };
    let probability = if probability.is_finite() {
        probability.clamp(0.0, 1.0)
    } else {
        // A partial frame, or a detector that would not answer: keep the last
        // value the meter had rather than flashing a false zero.
        monitor_probability(shared)
    };
    shared
        .monitor
        .level_bits
        .store(level_of(stored_output).to_bits(), Ordering::Relaxed);
    shared
        .monitor
        .probability_bits
        .store(probability.to_bits(), Ordering::Relaxed);
    shared
        .monitor
        .silence_ms
        .store(silence_ms, Ordering::Relaxed);
    shared.frames.fetch_add(1, Ordering::Relaxed);
}

fn monitor_probability(shared: &Shared) -> f32 {
    f32::from_bits(shared.monitor.probability_bits.load(Ordering::Relaxed))
}

/// Snap a meter reading to the resolution the pill can actually draw, so an
/// unchanged meter is bit-identical and can be skipped entirely.
fn quantise(value: f32, steps: f32) -> f32 {
    if !value.is_finite() {
        return 0.0;
    }
    ((value.clamp(0.0, 1.0) * steps).round() / steps).clamp(0.0, 1.0)
}

/// What to publish on the display channel, or nothing if it has not moved.
/// Pure, so the throttle rule is testable.
fn mic_level_update(
    previous: Option<(u32, u32)>,
    level: f32,
    probability: f32,
) -> Option<(f32, f32)> {
    let level = quantise(level, LEVEL_STEPS);
    let probability = quantise(probability, PROBABILITY_STEPS);
    let current = (level.to_bits(), probability.to_bits());
    (previous != Some(current)).then_some((level, probability))
}

/// Whether the level meter is due to move on this tick. Counted in ticks rather
/// than compared against a clock so it cannot be affected by a slow first tick.
fn level_due(ticks_since_emit: u32) -> bool {
    ticks_since_emit * MONITOR_TICK >= MIC_LEVEL_INTERVAL
}

fn start_monitor(shared: Arc<Shared>, app: AppHandle) -> JoinHandle<()> {
    thread::spawn(move || {
        let monitor = shared.monitor.clone();
        let mut emitted: Option<(u32, u32)> = None;
        let mut ticks_since_emit: u32 = 0;
        let mut frames = shared.frames.load(Ordering::Relaxed);
        let mut last_frame_at = Instant::now();
        while monitor.active.load(Ordering::Acquire) {
            ticks_since_emit = ticks_since_emit.saturating_add(1);
            if level_due(ticks_since_emit) {
                ticks_since_emit = 0;
                if let Some((level, probability)) =
                    mic_level_update(emitted, monitor.level(), monitor.probability())
                {
                    emitted = Some((level.to_bits(), probability.to_bits()));
                    let _ = app.emit("mic-level", (level, probability));
                }
            }

            // `limit_reached` is deliberately not tested here: reaching the
            // recording cap is a normal end of take, and folding it into the
            // stream-error branch would file a 20-minute take as a microphone
            // failure in the lifecycle log.
            if shared.failed.load(Ordering::Acquire) || STREAM_ERROR.load(Ordering::Acquire) {
                crate::crash::record_event("audio:stream-error");
                crate::dictation::stop(&app);
                break;
            }
            if shared.limit_reached.load(Ordering::Acquire) {
                crate::crash::record_event("audio:limit-reached");
                crate::dictation::stop(&app);
                break;
            }
            let seen = shared.frames.load(Ordering::Relaxed);
            if seen != frames {
                frames = seen;
                last_frame_at = Instant::now();
            } else if last_frame_at.elapsed() >= STALL_LIMIT {
                // The device stopped delivering. Without this the take would sit
                // on "listening" for ever: no frames means no silence, so the
                // auto-stop above can never fire either.
                crate::crash::record_event("audio:stream-stalled");
                mark_failed(&shared);
                crate::dictation::stop(&app);
                break;
            }
            if monitor.silence_ms.load(Ordering::Relaxed) > SILENCE_AUTO_STOP_MS
                && crate::dictation::is_recording()
            {
                // This is a normal worker thread, never the CPAL callback. The
                // stop path takes the stream and performs the state transition.
                crate::dictation::stop(&app);
                break;
            }
            thread::sleep(MONITOR_TICK);
        }
    })
}

pub fn start(app: AppHandle) -> Result<(), String> {
    crate::crash::record_event("audio:start-requested");
    let host = cpal::default_host();
    let device = resolve_device(&app)
        .or_else(|| host.default_input_device())
        .ok_or_else(|| "No microphone found.".to_string())?;
    let config = device
        .default_input_config()
        .map_err(|error| format!("Could not read microphone settings: {error}"))?;
    let sample_rate = config.sample_rate().0;
    let channels = config.channels() as usize;
    if sample_rate == 0 || channels == 0 {
        return Err("The microphone returned an invalid audio format.".into());
    }
    let format = config.sample_format();

    STREAM_ERROR.store(false, Ordering::Release);
    let monitor = Arc::new(MonitorState::new());
    let shared = Arc::new(Shared {
        // Reserved once, here, rather than grown inside the callback: reaching
        // the cap by doubling copies tens of megabytes on the audio thread, and
        // a single 76 MB reservation costs one allocation and no memmove.
        samples: Mutex::new(Vec::with_capacity(MAX_RECORDING_SAMPLES)),
        vad: Mutex::new(crate::dictation::vad::VadState::new()),
        frames: AtomicU64::new(0),
        monitor: monitor.clone(),
        failed: AtomicBool::new(false),
        limit_reached: AtomicBool::new(false),
    });

    let scratch = Scratch::new(sample_rate)?;
    let stream = build_stream(
        &device,
        &config.into(),
        format,
        channels,
        shared.clone(),
        scratch,
    )
    .map_err(|error| format!("Could not open the microphone: {error}"))?;

    stream
        .play()
        .map_err(|error| format!("Could not start microphone capture: {error}"))?;

    // Install the recorder before the monitor can observe silence. This closes
    // the old start/stop race where a quiet input could transition to
    // Transcribing before the stream was actually stored.
    let monitor_shared = shared.clone();
    *lock(&RECORDER) = Some(Recorder {
        _stream: stream,
        shared,
        monitor: None,
    });
    let recorder_monitor = start_monitor(monitor_shared, app);
    if let Some(recorder) = lock(&RECORDER).as_mut() {
        recorder.monitor = Some(recorder_monitor);
    }
    crate::crash::record_event("audio:started");
    Ok(())
}

fn build_stream(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    format: cpal::SampleFormat,
    channels: usize,
    shared: Arc<Shared>,
    mut scratch: Scratch,
) -> Result<cpal::Stream, String> {
    match format {
        cpal::SampleFormat::I16 => device
            .build_input_stream(
                config,
                move |data: &[i16], _| {
                    if quiet(&shared) {
                        return;
                    }
                    reserve_block(&mut scratch.block, data.len() / channels);
                    for frame in data.chunks_exact(channels) {
                        scratch.block.push(
                            frame.iter().map(|&s| s as f32 / 32768.0).sum::<f32>()
                                / channels as f32,
                        );
                    }
                    process_block(
                        &shared,
                        &mut scratch.resampler,
                        &mut scratch.fifo,
                        &mut scratch.frame,
                        &scratch.block,
                    );
                },
                err_cb,
                None,
            )
            .map_err(|error| error.to_string()),
        cpal::SampleFormat::U16 => device
            .build_input_stream(
                config,
                move |data: &[u16], _| {
                    if quiet(&shared) {
                        return;
                    }
                    reserve_block(&mut scratch.block, data.len() / channels);
                    for frame in data.chunks_exact(channels) {
                        scratch.block.push(
                            frame
                                .iter()
                                .map(|&s| (s as f32 / 32768.0) - 1.0)
                                .sum::<f32>()
                                / channels as f32,
                        );
                    }
                    process_block(
                        &shared,
                        &mut scratch.resampler,
                        &mut scratch.fifo,
                        &mut scratch.frame,
                        &scratch.block,
                    );
                },
                err_cb,
                None,
            )
            .map_err(|error| error.to_string()),
        _ => device
            .build_input_stream(
                config,
                move |data: &[f32], _| {
                    if quiet(&shared) {
                        return;
                    }
                    reserve_block(&mut scratch.block, data.len() / channels);
                    for frame in data.chunks_exact(channels) {
                        scratch
                            .block
                            .push(frame.iter().sum::<f32>() / channels as f32);
                    }
                    process_block(
                        &shared,
                        &mut scratch.resampler,
                        &mut scratch.fifo,
                        &mut scratch.frame,
                        &scratch.block,
                    );
                },
                err_cb,
                None,
            )
            .map_err(|error| error.to_string()),
    }
}

fn err_cb(_err: cpal::StreamError) {
    // CPAL invokes this on an audio thread. Keep it side-effect free; the
    // monitor observes this flag and performs the state transition normally.
    STREAM_ERROR.store(true, Ordering::Release);
}

pub struct RecordingResult {
    pub samples: Vec<f32>,
    pub speech_detected: bool,
    pub error: Option<String>,
}

pub fn stop() -> RecordingResult {
    crate::crash::record_event("audio:stop-requested");
    let recorder = lock(&RECORDER).take();
    let Some(recorder) = recorder else {
        return RecordingResult {
            samples: Vec::new(),
            speech_detected: false,
            error: None,
        };
    };
    let Recorder {
        _stream,
        shared,
        monitor,
    } = recorder;
    shared.monitor.active.store(false, Ordering::Release);
    drop(_stream);
    if let Some(monitor) = monitor {
        let _ = monitor.join();
    }
    // Give the backend a moment to finish its final callback before reading the
    // shared samples. The monitor is already stopped, so this cannot wait on
    // Tauri or the UI thread.
    thread::sleep(Duration::from_millis(40));
    let speech_detected = lock(&shared.vad).speech_detected();
    let samples = std::mem::take(&mut *lock(&shared.samples));
    let error = (shared.failed.load(Ordering::Acquire) || STREAM_ERROR.load(Ordering::Acquire))
        .then(|| {
            "Microphone capture failed. Check the selected input device and try again.".to_string()
        });
    crate::crash::record_event("audio:stopped");
    RecordingResult {
        samples,
        speech_detected,
        error,
    }
}

pub fn cancel() {
    STREAM_ERROR.store(false, Ordering::Release);
    if let Some(recorder) = lock(&RECORDER).take() {
        recorder
            .shared
            .monitor
            .active
            .store(false, Ordering::Release);
        drop(recorder._stream);
        if let Some(monitor) = recorder.monitor {
            let _ = monitor.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recording_limit_is_twenty_minutes() {
        assert_eq!(MAX_RECORDING_MINUTES, 20);
        assert_eq!(MAX_RECORDING_SAMPLES, TARGET_RATE as usize * 60 * 20);
    }

    #[test]
    fn bounded_append_keeps_the_limit_and_reports_boundary() {
        let mut samples = vec![1.0, 2.0, 3.0];
        let (stored, limited) = append_bounded_samples(&mut samples, &[4.0, 5.0, 6.0], 4);
        assert_eq!(stored, 1);
        assert!(limited);
        assert_eq!(samples, vec![1.0, 2.0, 3.0, 4.0]);

        let (stored, limited) = append_bounded_samples(&mut samples, &[7.0], 4);
        assert_eq!(stored, 0);
        assert!(limited);
        assert_eq!(samples.len(), 4);
    }

    #[test]
    fn bounded_append_does_not_report_a_limit_before_it_is_reached() {
        let mut samples = vec![1.0, 2.0];
        let (stored, limited) = append_bounded_samples(&mut samples, &[3.0, 4.0], 4);
        assert_eq!(stored, 2);
        assert!(!limited);
        assert_eq!(samples, vec![1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn the_frame_is_the_frame_the_vad_expects() {
        // A different chunk size does not degrade the VAD, it breaks it:
        // earshot's predict_f32 answers -1.0 for any length but 256.
        assert_eq!(CHUNK_OUT, crate::dictation::vad::FRAME_SAMPLES);
        assert_eq!(CHUNK_OUT as u64 * crate::dictation::vad::FRAME_MS, 256 * 16);
        assert_eq!(TARGET_RATE, 16_000);
    }

    #[test]
    fn the_scratch_buffers_are_allocated_before_the_stream_exists() {
        let mut scratch = Scratch::new(48_000).expect("48 kHz is a supported rate");
        assert_eq!(scratch.fifo.capacity(), MAX_INPUT_BUFFER_SAMPLES);
        assert_eq!(scratch.frame.len(), CHUNK_OUT);
        // block has no known size until the first callback, and then keeps it.
        reserve_block(&mut scratch.block, 480);
        reserve_block(&mut scratch.block, 512);
        reserve_block(&mut scratch.block, 64);
        assert!(scratch.block.capacity() >= 512);
    }

    #[test]
    fn an_unchanged_meter_emits_nothing() {
        let first = mic_level_update(None, 0.5, 0.25).expect("the first reading is always sent");
        assert_eq!(first, (0.5, 0.25));
        let last = Some((first.0.to_bits(), first.1.to_bits()));
        // 20 messages a second, 20 minutes, and the only reading ever sent.
        for _ in 0..2_400 {
            assert_eq!(mic_level_update(last, 0.5, 0.25), None);
        }
    }

    #[test]
    fn the_meter_is_quantised_to_what_the_pill_can_draw() {
        // Anything inside a step is the same reading, so it must not move.
        let a = mic_level_update(None, 0.100, 0.100).expect("first");
        let last = Some((a.0.to_bits(), a.1.to_bits()));
        assert_eq!(mic_level_update(last, 0.102, 0.100), None);
        assert_eq!(mic_level_update(last, 0.098, 0.100), None);
        // A real move is published.
        assert!(mic_level_update(last, 0.20, 0.10).is_some());
        // And every published value is on the grid and in range.
        let (level, probability) = mic_level_update(None, 0.3333, 0.7777).expect("first");
        assert!((0.0..=1.0).contains(&level));
        assert!((0.0..=1.0).contains(&probability));
        assert!((level * LEVEL_STEPS - (level * LEVEL_STEPS).round()).abs() < 1e-4);
        assert!(
            (probability * PROBABILITY_STEPS - (probability * PROBABILITY_STEPS).round()).abs()
                < 1e-4
        );
    }

    #[test]
    fn a_malformed_meter_reading_publishes_zero_rather_than_nonsense() {
        let (level, probability) = mic_level_update(None, f32::NAN, f32::INFINITY).expect("first");
        assert_eq!(level, 0.0);
        assert_eq!(probability, 0.0);
        let (level, probability) = mic_level_update(None, -3.0, 9.0).expect("first");
        assert_eq!(level, 0.0);
        assert_eq!(probability, 1.0);
    }

    #[test]
    fn the_level_meter_is_throttled_and_nothing_else_is() {
        // Four 50 ms ticks per published reading: 5 a second, not 20.
        assert!(!level_due(1));
        assert!(!level_due(2));
        assert!(!level_due(3));
        assert!(level_due(4));
        assert_eq!(MONITOR_TICK * 4, MIC_LEVEL_INTERVAL);
        // A 20-minute take at the old 50 ms cadence was 24,000 renders.
        let old = 20 * 60 * 1000 / MONITOR_TICK.as_millis();
        let now = 20 * 60 * 1000 / MIC_LEVEL_INTERVAL.as_millis();
        assert_eq!(old, 24_000);
        assert_eq!(now, 6_000);
        // The control tick is unchanged, so the auto-stop is as responsive as
        // it ever was.
        assert_eq!(MONITOR_TICK, Duration::from_millis(50));
    }

    #[test]
    fn the_auto_stop_gap_is_the_hangover_plus_the_silence_window() {
        // The two policies compose: 1.6 s of silence is measured from the end of
        // the 600 ms hangover, so the real gap after the last word is 2.2 s.
        // audio::SILENCE_AUTO_STOP_MS and vad::HANGOVER_MS have to agree or the
        // documented 1.6 s is not 1.6 s.
        let mut policy = crate::dictation::vad::VadPolicy::default();
        for _ in 0..4 {
            policy.feed(0.9);
        }
        let mut since_word = 0u64;
        while policy.silence_ms() <= SILENCE_AUTO_STOP_MS {
            policy.feed(0.0);
            since_word += crate::dictation::vad::FRAME_MS;
            assert!(since_word < 10_000, "the hangover never released");
        }
        assert_eq!(
            since_word,
            crate::dictation::vad::HANGOVER_MS + SILENCE_AUTO_STOP_MS
        );
    }
}
