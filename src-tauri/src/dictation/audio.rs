use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rubato::{FftFixedOut, Resampler};
use tauri::{AppHandle, Emitter};

const TARGET_RATE: u32 = 16000;
const CHUNK_OUT: usize = 256;
const SILENCE_AUTO_STOP_MS: u64 = 1600;

struct Shared {
    samples: Mutex<Vec<f32>>,
    vad: Mutex<crate::dictation::vad::VadState>,
    silence_ms: Mutex<u64>,
    resampler: Mutex<Option<FftFixedOut<f32>>>,
    in_buf: Mutex<Vec<f32>>,
}

static RECORDER: Mutex<Option<Recorder>> = Mutex::new(None);

struct Recorder {
    _stream: cpal::Stream,
    shared: Arc<Shared>,
}

pub fn mic_ready() -> bool {
    cpal::default_host()
        .default_input_device()
        .is_some()
}

pub fn list_input_devices() -> Vec<String> {
    let host = cpal::default_host();
    host.input_devices()
        .map(|devs| {
            devs.filter_map(|d| d.name().ok())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn resolve_device(app: &AppHandle) -> Option<cpal::Device> {
    let wanted = crate::db::get_setting(app, "selected_microphone")?;
    if wanted.is_empty() {
        return None;
    }
    let host = cpal::default_host();
    host.input_devices()
        .ok()
        .and_then(|devs| {
            devs.into_iter().find(|d| d.name().ok().as_deref() == Some(wanted.as_str()))
        })
}

fn process_chunk(shared: &Shared, app: &AppHandle, sample_rate: u32) {
    let mut res_guard = shared.resampler.lock().unwrap();
    let resampler = res_guard.get_or_insert_with(|| {
        FftFixedOut::<f32>::new(
            sample_rate as usize,
            TARGET_RATE as usize,
            CHUNK_OUT,
            1,
            1,
        )
        .expect("resampler")
    });
    let needed = resampler.input_frames_next();
    let mut in_guard = shared.in_buf.lock().unwrap();
    if in_guard.len() < needed {
        return;
    }
    let input: Vec<f32> = in_guard.drain(..needed).collect();
    drop(in_guard);

    let mut out_buf = vec![0.0f32; CHUNK_OUT];
    let _ = resampler.process_into_buffer(&[input], &mut [out_buf.as_mut_slice()], None);
    drop(res_guard);

    let mut samples_guard = shared.samples.lock().unwrap();
    samples_guard.extend_from_slice(&out_buf);
    drop(samples_guard);

    let (prob, silence) = {
        let mut vad = shared.vad.lock().unwrap();
        let p = vad.feed(&out_buf);
        let sil = vad.silence_ms;
        *shared.silence_ms.lock().unwrap() = sil;
        (p, sil)
    };
    let level = {
        let vad = shared.vad.lock().unwrap();
        vad.level(&out_buf)
    };
    let _ = app.emit("mic-level", (level, prob));
    if crate::dictation::is_recording() && silence > SILENCE_AUTO_STOP_MS {
        crate::dictation::stop(app);
    }
}

pub fn start(app: AppHandle) {
    let host = cpal::default_host();
    let device = resolve_device(&app).or_else(|| host.default_input_device());
    let Some(device) = device else {
        return;
    };
    let Ok(config) = device.default_input_config() else {
        return;
    };
    let sample_rate = config.sample_rate().0;
    let channels = config.channels() as usize;
    let format = config.sample_format();

    let shared = Arc::new(Shared {
        samples: Mutex::new(Vec::new()),
        vad: Mutex::new(crate::dictation::vad::VadState::new()),
        silence_ms: Mutex::new(0),
        resampler: Mutex::new(None),
        in_buf: Mutex::new(Vec::new()),
    });

    let stream = build_stream(
        &device,
        &config.into(),
        format,
        channels,
        sample_rate,
        shared.clone(),
        app.clone(),
    );

    match stream {
        Ok(stream) => {
            if let Err(e) = stream.play() {
                log::warn!("stream play failed: {e}");
            }
            *RECORDER.lock().unwrap() = Some(Recorder { _stream: stream, shared });
        }
        Err(e) => {
            log::error!("failed to build input stream: {e}");
        }
    }
}

fn build_stream(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    format: cpal::SampleFormat,
    channels: usize,
    sample_rate: u32,
    shared: Arc<Shared>,
    app: AppHandle,
) -> Result<cpal::Stream, cpal::BuildStreamError> {
    let push = move |mono: &[f32]| {
        shared.in_buf.lock().unwrap().extend_from_slice(mono);
        process_chunk(&shared, &app, sample_rate);
    };
    match format {
        cpal::SampleFormat::I16 => device.build_input_stream(
            config,
            move |data: &[i16], _| {
                let mono: Vec<f32> = data
                    .chunks(channels)
                    .map(|c| c.iter().map(|&s| s as f32 / 32768.0).sum::<f32>() / c.len() as f32)
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
                    .map(|c| {
                        c.iter()
                            .map(|&s| (s as f32 / 32768.0) - 1.0)
                            .sum::<f32>()
                            / c.len() as f32
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
                    .map(|c| c.iter().sum::<f32>() / c.len() as f32)
                    .collect();
                push(&mono);
            },
            err_cb,
            None,
        ),
    }
}

fn err_cb(err: cpal::StreamError) {
    log::error!("audio stream error: {err}");
}

pub fn stop() -> Vec<f32> {
    let rec = RECORDER.lock().unwrap().take();
    if let Some(rec) = rec {
        let Recorder { _stream, shared } = rec;
        drop(_stream);
        std::thread::sleep(Duration::from_millis(40));
        let out = std::mem::take(&mut *shared.samples.lock().unwrap());
        drop(shared);
        out
    } else {
        Vec::new()
    }
}

pub fn cancel() {
    *RECORDER.lock().unwrap() = None;
}