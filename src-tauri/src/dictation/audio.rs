use std::{
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        Arc, Mutex, MutexGuard,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rubato::{FftFixedOut, Resampler};
use tauri::{AppHandle, Emitter};

const TARGET_RATE: u32 = 16000;
const CHUNK_OUT: usize = 256;
const SILENCE_AUTO_STOP_MS: u64 = 1600;
const MONITOR_INTERVAL: Duration = Duration::from_millis(50);
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

struct Shared {
    samples: Mutex<Vec<f32>>,
    vad: Mutex<crate::dictation::vad::VadState>,
    resampler: Mutex<Option<FftFixedOut<f32>>>,
    in_buf: Mutex<Vec<f32>>,
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

pub fn mic_ready() -> bool {
    cpal::default_host().default_input_device().is_some()
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

fn process_chunk(shared: &Shared, sample_rate: u32) {
    if shared.failed.load(Ordering::Acquire) || shared.limit_reached.load(Ordering::Acquire) {
        return;
    }
    if sample_rate == 0 {
        mark_failed(shared);
        return;
    }

    let mut resampler_guard = lock(&shared.resampler);
    if resampler_guard.is_none() {
        let resampler = match FftFixedOut::<f32>::new(
            sample_rate as usize,
            TARGET_RATE as usize,
            CHUNK_OUT,
            1,
            1,
        ) {
            Ok(resampler) => resampler,
            Err(_) => {
                mark_failed(shared);
                return;
            }
        };
        *resampler_guard = Some(resampler);
    }
    let Some(resampler) = resampler_guard.as_mut() else {
        return;
    };
    let needed = resampler.input_frames_next();
    let mut input_guard = lock(&shared.in_buf);
    if input_guard.len() < needed {
        return;
    }
    let input: Vec<f32> = input_guard.drain(..needed).collect();
    drop(input_guard);

    let mut output = vec![0.0f32; CHUNK_OUT];
    if resampler
        .process_into_buffer(&[input], &mut [output.as_mut_slice()], None)
        .is_err()
    {
        mark_failed(shared);
        return;
    }
    drop(resampler_guard);

    let stored_len = {
        let mut samples = lock(&shared.samples);
        append_bounded_samples(&mut samples, &output, MAX_RECORDING_SAMPLES).0
    };
    if stored_len < output.len() {
        shared.limit_reached.store(true, Ordering::Release);
    }
    let stored_output = &output[..stored_len];

    let (probability, silence_ms) = {
        let mut vad = lock(&shared.vad);
        let probability = vad.feed(stored_output);
        (probability, vad.silence_ms)
    };
    let probability = if probability.is_finite() {
        probability.clamp(0.0, 1.0)
    } else {
        0.0
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
}

fn start_monitor(shared: Arc<Shared>, app: AppHandle) -> JoinHandle<()> {
    thread::spawn(move || {
        let monitor = shared.monitor.clone();
        while monitor.active.load(Ordering::Acquire) {
            let level = monitor.level();
            let probability = monitor.probability();
            let _ = app.emit("mic-level", (level, probability));

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
            if monitor.silence_ms.load(Ordering::Relaxed) > SILENCE_AUTO_STOP_MS
                && crate::dictation::is_recording()
            {
                // This is a normal worker thread, never the CPAL callback. The
                // stop path takes the stream and performs the state transition.
                crate::dictation::stop(&app);
                break;
            }
            thread::sleep(MONITOR_INTERVAL);
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
        samples: Mutex::new(Vec::new()),
        vad: Mutex::new(crate::dictation::vad::VadState::new()),
        resampler: Mutex::new(None),
        in_buf: Mutex::new(Vec::new()),
        monitor: monitor.clone(),
        failed: AtomicBool::new(false),
        limit_reached: AtomicBool::new(false),
    });

    let stream = build_stream(
        &device,
        &config.into(),
        format,
        channels,
        sample_rate,
        shared.clone(),
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
    sample_rate: u32,
    shared: Arc<Shared>,
) -> Result<cpal::Stream, cpal::BuildStreamError> {
    let push = move |mono: &[f32]| {
        if shared.failed.load(Ordering::Acquire) || shared.limit_reached.load(Ordering::Acquire) {
            return;
        }
        {
            let mut input = lock(&shared.in_buf);
            if input.len().saturating_add(mono.len()) > MAX_INPUT_BUFFER_SAMPLES {
                mark_failed(&shared);
                return;
            }
            input.extend_from_slice(mono);
        }
        process_chunk(&shared, sample_rate);
    };
    match format {
        cpal::SampleFormat::I16 => device.build_input_stream(
            config,
            move |data: &[i16], _| {
                let mono: Vec<f32> = data
                    .chunks(channels)
                    .map(|channel| {
                        channel
                            .iter()
                            .map(|&sample| sample as f32 / 32768.0)
                            .sum::<f32>()
                            / channel.len() as f32
                    })
                    .collect();
                push(&mono);
            },
            err_cb,
            None,
        ),
        cpal::SampleFormat::U16 => device.build_input_stream(
            config,
            move |data: &[u16], _| {
                let mono: Vec<f32> = data
                    .chunks(channels)
                    .map(|channel| {
                        channel
                            .iter()
                            .map(|&sample| (sample as f32 / 32768.0) - 1.0)
                            .sum::<f32>()
                            / channel.len() as f32
                    })
                    .collect();
                push(&mono);
            },
            err_cb,
            None,
        ),
        _ => device.build_input_stream(
            config,
            move |data: &[f32], _| {
                let mono: Vec<f32> = data
                    .chunks(channels)
                    .map(|channel| channel.iter().sum::<f32>() / channel.len() as f32)
                    .collect();
                push(&mono);
            },
            err_cb,
            None,
        ),
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
}
