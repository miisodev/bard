//! Internal engine benchmark. Input is mono 16kHz little-endian f32 PCM.
//! Reports timings and output length; never logs the transcript.
use std::{error::Error, fs, path::Path, time::Instant};
use transcribe_cpp::{Model, RunOptions, SessionOptions};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 4 {
        return Err("Usage: bard-asr-bench MODEL.gguf AUDIO.f32le THREADS".into());
    }
    let threads: i32 = args[3].parse()?;
    if !(1..=16).contains(&threads) {
        return Err("Threads must be between 1 and 16".into());
    }
    let pcm = fs::read(&args[2])?;
    if pcm.is_empty() || pcm.len() % 4 != 0 || pcm.len() > 16_000 * 4 * 60 {
        return Err("Provide up to one minute of mono16kHz f32 PCM".into());
    }
    let samples: Vec<f32> = pcm
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
        .collect();
    if samples.iter().any(|v| !v.is_finite() || v.abs() > 1.0) {
        return Err("Invalid PCM samples".into());
    }
    let start = Instant::now();
    transcribe_cpp::init_backends_default()?;
    let model = Model::load(Path::new(&args[1]))?;
    let mut session = model.session_with(&SessionOptions {
        n_threads: threads,
        ..Default::default()
    })?;
    let load_ms = start.elapsed().as_secs_f64() * 1000.0;
    let mut runs = Vec::new();
    for index in 0..4 {
        let start = Instant::now();
        let result = session.run(&samples, &RunOptions::default())?;
        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
        let characters = result.text.trim().chars().count();
        if characters == 0 {
            return Err("The synthetic speech produced an empty transcript".into());
        }
        runs.push(serde_json::json!({ "index": index, "elapsed_ms": elapsed_ms, "characters": characters }));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "engine": "transcribe-cpp 0.2.3", "backend": model.backend(),
            "threads": threads, "audio_seconds": samples.len() as f64 / 16_000.0,
            "load_ms": load_ms, "runs": runs,
            "scope": "engine inference only; excludes microphone, review and paste-back"
        }))?
    );
    Ok(())
}
