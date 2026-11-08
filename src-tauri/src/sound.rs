//! Bard's soft cues, synthesised on the spot and played through the default
//! output device.
//!
//! They are made in Rust rather than in a webview so they sound whether or
//! not any Bard window is open, and so a hidden, throttled webview can never
//! delay one. Each cue is a few tens of milliseconds of sine with a fast
//! attack and an exponential tail: a pop, a tick, a two-note rise. Nothing is
//! loaded from disk.
//!
//! One worker thread owns the output stream. It opens the device on the first
//! cue and closes it after a short idle, so Bard does not hold the audio
//! device (or appear in the system mixer) between cues.

use std::collections::VecDeque;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// The sounds Bard makes. Every one has a visual twin in the pill.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Cue {
    /// Something you copied was kept.
    Copy,
    /// Bard put an item on the clipboard for you.
    Select,
    /// The clipboard preview opened.
    Preview,
    /// Dictation started listening.
    Listen,
    /// Dictation stopped listening and is working.
    Stop,
    /// A transcript is on the clipboard.
    Done,
    /// Capture paused.
    Pause,
    /// Capture resumed.
    Resume,
    /// Something went wrong.
    Error,
    /// A neutral acknowledgement.
    Info,
}

impl Cue {
    pub fn from_kind(kind: &str) -> Cue {
        match kind {
            "copy" => Cue::Copy,
            "select" => Cue::Select,
            "preview" => Cue::Preview,
            "listen" => Cue::Listen,
            "work" => Cue::Stop,
            "done" => Cue::Done,
            "pause" => Cue::Pause,
            "resume" => Cue::Resume,
            "error" => Cue::Error,
            _ => Cue::Info,
        }
    }
}

/// One partial: a sine that glides from `from` to `to` Hz, starting `at`
/// seconds into the cue, with a 4 ms attack and an exponential decay of time
/// constant `tau`, lasting `length` seconds.
#[derive(Clone, Copy)]
struct Tone {
    at: f32,
    length: f32,
    from: f32,
    to: f32,
    tau: f32,
    gain: f32,
}

const fn tone(at: f32, length: f32, from: f32, to: f32, tau: f32, gain: f32) -> Tone {
    Tone {
        at,
        length,
        from,
        to,
        tau,
        gain,
    }
}

fn recipe(cue: Cue) -> &'static [Tone] {
    match cue {
        // A rounded pop: a quick downward glide with a quiet octave on top.
        Cue::Copy => {
            const {
                &[
                    tone(0.0, 0.09, 880.0, 520.0, 0.024, 0.20),
                    tone(0.0, 0.05, 1760.0, 1040.0, 0.010, 0.05),
                ]
            }
        }
        // A small dry tick.
        Cue::Select => {
            const {
                &[
                    tone(0.0, 0.04, 1500.0, 1150.0, 0.008, 0.16),
                    tone(0.0, 0.03, 3000.0, 2300.0, 0.004, 0.03),
                ]
            }
        }
        Cue::Preview => const { &[tone(0.0, 0.10, 620.0, 780.0, 0.030, 0.15)] },
        Cue::Listen => {
            const {
                &[
                    tone(0.0, 0.08, 660.0, 660.0, 0.030, 0.14),
                    tone(0.075, 0.11, 990.0, 990.0, 0.040, 0.14),
                ]
            }
        }
        Cue::Stop => const { &[tone(0.0, 0.10, 760.0, 560.0, 0.030, 0.13)] },
        Cue::Done => {
            const {
                &[
                    tone(0.0, 0.09, 1046.0, 700.0, 0.026, 0.18),
                    tone(0.0, 0.05, 2092.0, 1400.0, 0.010, 0.04),
                ]
            }
        }
        Cue::Pause => {
            const {
                &[
                    tone(0.0, 0.08, 700.0, 700.0, 0.028, 0.13),
                    tone(0.07, 0.10, 470.0, 470.0, 0.034, 0.13),
                ]
            }
        }
        Cue::Resume => {
            const {
                &[
                    tone(0.0, 0.08, 470.0, 470.0, 0.028, 0.13),
                    tone(0.07, 0.10, 700.0, 700.0, 0.034, 0.13),
                ]
            }
        }
        // Two low, soft knocks: noticeable, never alarming.
        Cue::Error => {
            const {
                &[
                    tone(0.0, 0.10, 330.0, 300.0, 0.035, 0.17),
                    tone(0.13, 0.12, 300.0, 270.0, 0.040, 0.17),
                ]
            }
        }
        Cue::Info => const { &[tone(0.0, 0.05, 1200.0, 1000.0, 0.012, 0.12)] },
    }
}

/// Render a cue as mono samples at `rate` Hz.
fn render(cue: Cue, rate: u32) -> Vec<f32> {
    let rate = rate.max(8_000) as f32;
    let tones = recipe(cue);
    let end = tones.iter().map(|t| t.at + t.length).fold(0.0f32, f32::max);
    let total = (end * rate).ceil() as usize + 1;
    let mut out = vec![0.0f32; total];
    for t in tones {
        let start = (t.at * rate) as usize;
        let count = (t.length * rate) as usize;
        let mut phase = 0.0f32;
        for n in 0..count {
            let time = n as f32 / rate;
            let progress = time / t.length;
            // Exponential glide sounds even to the ear; a linear one does not.
            let freq = t.from * (t.to / t.from).powf(progress);
            phase += std::f32::consts::TAU * freq / rate;
            let attack = (time / 0.004).min(1.0);
            // A short fade at the very end so no partial ever clicks off.
            let release = ((t.length - time) / 0.006).clamp(0.0, 1.0);
            let envelope = attack * (-time / t.tau).exp() * release;
            if let Some(sample) = out.get_mut(start + n) {
                *sample += phase.sin() * envelope * t.gain;
            }
        }
    }
    for sample in &mut out {
        *sample = sample.clamp(-0.9, 0.9);
    }
    out
}

fn sender() -> &'static Mutex<Sender<Cue>> {
    static SENDER: OnceLock<Mutex<Sender<Cue>>> = OnceLock::new();
    SENDER.get_or_init(|| {
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("bard-sound".into())
            .spawn(move || worker(rx))
            .ok();
        Mutex::new(tx)
    })
}

/// Play a cue. Never blocks and never fails loudly: a machine with no output
/// device simply stays quiet, and the pill still says what happened.
pub fn play(cue: Cue) {
    let tx = sender()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _ = tx.send(cue);
}

/// How long the stream stays open after the last cue.
const IDLE_CLOSE: Duration = Duration::from_secs(4);

fn worker(rx: Receiver<Cue>) {
    while let Ok(first) = rx.recv() {
        let Some(output) = Output::open() else {
            crate::crash::record_event("sound:no-output-device");
            // Drain what queued meanwhile; there is nothing to play it on.
            while rx.try_recv().is_ok() {}
            continue;
        };
        output.push(render(first, output.rate));
        loop {
            match rx.recv_timeout(IDLE_CLOSE) {
                Ok(cue) => output.push(render(cue, output.rate)),
                Err(RecvTimeoutError::Timeout) => {
                    if output.idle() {
                        break;
                    }
                }
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    }
}

struct Output {
    _stream: cpal::Stream,
    queue: Arc<Mutex<VecDeque<f32>>>,
    rate: u32,
}

impl Output {
    fn open() -> Option<Output> {
        let host = cpal::default_host();
        let device = host.default_output_device()?;
        let supported = device.default_output_config().ok()?;
        let format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();
        let rate = config.sample_rate.0;
        let channels = config.channels as usize;
        let queue: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
        let stream = match format {
            cpal::SampleFormat::F32 => build::<f32>(&device, &config, channels, queue.clone()),
            cpal::SampleFormat::I16 => build::<i16>(&device, &config, channels, queue.clone()),
            cpal::SampleFormat::U16 => build::<u16>(&device, &config, channels, queue.clone()),
            cpal::SampleFormat::I32 => build::<i32>(&device, &config, channels, queue.clone()),
            _ => None,
        }?;
        stream.play().ok()?;
        Some(Output {
            _stream: stream,
            queue,
            rate,
        })
    }

    fn push(&self, samples: Vec<f32>) {
        let mut queue = self
            .queue
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        // Overlapping cues mix rather than queue, so a burst of copies sounds
        // like a burst and not like a backlog.
        for (index, sample) in samples.into_iter().enumerate() {
            match queue.get_mut(index) {
                Some(existing) => *existing = (*existing + sample).clamp(-0.9, 0.9),
                None => queue.push_back(sample),
            }
        }
    }

    fn idle(&self) -> bool {
        self.queue
            .lock()
            .map(|queue| queue.is_empty())
            .unwrap_or(true)
    }
}

fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: usize,
    queue: Arc<Mutex<VecDeque<f32>>>,
) -> Option<cpal::Stream>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                // try_lock: the audio thread must never wait. A frame that
                // finds the queue busy plays silence and catches up next time.
                let mut queue = queue.try_lock().ok();
                for frame in data.chunks_mut(channels.max(1)) {
                    let sample = queue.as_mut().and_then(|q| q.pop_front()).unwrap_or(0.0);
                    for out in frame.iter_mut() {
                        *out = T::from_sample(sample);
                    }
                }
            },
            |_| crate::crash::record_event("sound:stream-error"),
            None,
        )
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_cue_renders_short_and_bounded() {
        for cue in [
            Cue::Copy,
            Cue::Select,
            Cue::Preview,
            Cue::Listen,
            Cue::Stop,
            Cue::Done,
            Cue::Pause,
            Cue::Resume,
            Cue::Error,
            Cue::Info,
        ] {
            let samples = render(cue, 48_000);
            assert!(!samples.is_empty());
            assert!(
                samples.len() < 48_000 / 2,
                "{cue:?} is longer than half a second"
            );
            assert!(samples.iter().all(|s| s.abs() <= 0.9));
            assert!(samples.iter().any(|s| s.abs() > 0.01), "{cue:?} is silent");
        }
    }

    #[test]
    fn a_cue_ends_in_silence() {
        let samples = render(Cue::Copy, 44_100);
        assert!(samples.last().unwrap().abs() < 1e-3);
    }

    #[test]
    fn kinds_map_to_cues() {
        assert_eq!(Cue::from_kind("copy"), Cue::Copy);
        assert_eq!(Cue::from_kind("error"), Cue::Error);
        assert_eq!(Cue::from_kind("anything"), Cue::Info);
    }
}
