# Bard ASR benchmark

Internal performance tool using the application's pinned transcribe-cpp Rust
wrapper and native engine. Build on CI, then run the binary with a catalogue
GGUF, mono 16kHz f32 little-endian PCM and a thread count:

```sh
bard-asr-bench MODEL.gguf speech.f32le 4
```

The JSON reports model/session load time, four inference runs and character
counts. The first run is cold; the following three reuse the session. It does
not log transcript content. These measurements exclude microphone capture,
review and paste-back, and do not establish the complete app's latency budget.
Record the CPU, memory cap, CPU quota and model checksum alongside results.

`speech.f32le` is a ten-second synthetic fixture, generated with FFmpeg's flite
filter from this original text: “Bard keeps useful words close by. Speak a
thought and keep working. This short recording checks local dictation on the
computer.” It contains no person's recording. The file is mono 16kHz f32le,
160,000 samples, 640,000 bytes; zero padding/trimming makes it exactly ten seconds.

Keep local runs isolated and bounded so the benchmark cannot exhaust the shared
desktop. Compilation and browser rendering belong on hosted CI.
