use earshot::Detector;

const ONSET_MS: u64 = 60;
const HANGOVER_MS: u64 = 600;

pub struct VadState {
    detector: Box<Detector>,
    in_speech: bool,
    onset_ms: u64,
    hangover_ms: u64,
    pub silence_ms: u64,
}

impl VadState {
    pub fn new() -> Self {
        Self {
            detector: Detector::default_boxed(),
            in_speech: false,
            onset_ms: 0,
            hangover_ms: 0,
            silence_ms: 0,
        }
    }

    pub fn feed(&mut self, frame: &[f32]) -> f32 {
        let prob = self.detector.predict_f32(frame);
        if prob >= 0.35 {
            self.silence_ms = 0;
            self.hangover_ms = HANGOVER_MS;
            if !self.in_speech {
                self.onset_ms += 16;
                if self.onset_ms >= ONSET_MS {
                    self.in_speech = true;
                    self.onset_ms = 0;
                }
            }
        } else if self.in_speech {
            self.hangover_ms = self.hangover_ms.saturating_sub(16);
            if self.hangover_ms == 0 {
                self.in_speech = false;
            }
        } else {
            self.onset_ms = 0;
        }
        if !self.in_speech {
            self.silence_ms = self.silence_ms.saturating_add(16);
        }
        prob
    }

    pub fn level(&self, frame: &[f32]) -> f32 {
        let rms = (frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32).sqrt();
        (rms * 6.0).min(1.0)
    }
}