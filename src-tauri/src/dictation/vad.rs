use earshot::Detector;

/// earshot's Silero VAD consumes exactly 256 samples at 16 kHz, so one frame is
/// 16 ms and every timing below is counted in whole frames. `audio::CHUNK_OUT`
/// must stay at this length: `predict_f32` returns -1.0 for any other frame
/// size, so a resampler change that moved it would not degrade the VAD, it would
/// disable speech detection outright with no error anywhere.
pub const FRAME_SAMPLES: usize = 256;
pub const FRAME_MS: u64 = 16;

/// Speech has to hold above threshold for this long before it counts. One 16 ms
/// frame is a mouse click, a keypress or a cough; four of them is a syllable.
const ONSET_MS: u64 = 60;

/// Once speech is confirmed, this long below threshold keeps the take open, so
/// a pause between words is not the end of the sentence. 600 ms is not a whole
/// number of 16 ms frames, so the real hangover is 38 frames — 608 ms.
pub const HANGOVER_FRAMES: u64 = 38;
pub const HANGOVER_MS: u64 = HANGOVER_FRAMES * FRAME_MS;

/// How long a live take keeps running on its own after the hangover releases.
/// This is measured from the *end* of the hangover, so the real gap between the
/// last syllable and an auto-stopped take is `HANGOVER_MS + 1600` — 2.2 s, not
/// 1.6 s. The two constants are separate mechanisms and were always composed;
/// the comment and the tests are what stop that from drifting apart silently.
const SPEECH_THRESHOLD: f32 = 0.35;

/// The timing half of the detector, with no model in it.
///
/// This is where every number the user can feel lives, so it is separated from
/// the neural network and tested on its own. Before this existed the policy was
/// a dozen lines buried in a `feed` that also ran an FFT, and none of it was
/// reachable from a test.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VadPolicy {
    in_speech: bool,
    onset_ms: u64,
    hangover_ms: u64,
    confirmed: bool,
    silence_ms: u64,
}

impl VadPolicy {
    /// Advance one frame's speech probability.
    pub fn feed(&mut self, probability: f32) {
        if probability >= SPEECH_THRESHOLD {
            self.silence_ms = 0;
            self.hangover_ms = HANGOVER_MS;
            if self.in_speech {
                self.onset_ms = 0;
            } else {
                self.onset_ms += FRAME_MS;
                if self.onset_ms >= ONSET_MS {
                    self.in_speech = true;
                    // Only now does the take count as having heard something.
                    // Setting this on the first frame above threshold is what let
                    // a single 16 ms click past the "Nothing heard" gate and into
                    // a transcription of near-silence, which is how a whisper
                    // model gets invited to hallucinate.
                    self.confirmed = true;
                    self.onset_ms = 0;
                }
            }
        } else if self.in_speech {
            self.onset_ms = 0;
            self.hangover_ms = self.hangover_ms.saturating_sub(FRAME_MS);
            if self.hangover_ms == 0 {
                self.in_speech = false;
            }
        } else {
            self.onset_ms = 0;
        }
        if !self.in_speech {
            self.silence_ms = self.silence_ms.saturating_add(FRAME_MS);
        }
    }

    /// True once a full onset has been heard. This is all `speech_detected`
    /// means: the take has something in it worth transcribing.
    pub fn confirmed(&self) -> bool {
        self.confirmed
    }

    pub fn silence_ms(&self) -> u64 {
        self.silence_ms
    }
}

pub struct VadState {
    detector: Box<Detector>,
    policy: VadPolicy,
}

impl VadState {
    /// A fresh detector per recording, deliberately.
    ///
    /// The Silero weights are `include_bytes!` inside earshot, so this is not a
    /// weight load: it is one heap allocation of about 8 KiB — `Detector` holds a
    /// 768-sample ring buffer, a 1026-float FFT scratch and the GRU state — plus
    /// the memset that zeroes it. That is microseconds, once per take, and it is
    /// noise beside opening the microphone or loading a model.
    ///
    /// Hoisting it behind a `OnceLock` would save nothing worth having and cost
    /// correctness: `Detector` carries the GRU state and the ring buffer, so one
    /// detector shared between takes would open the next take with the previous
    /// one's tail still in it. `reset()` exists, but sharing would also put the
    /// audio thread behind a lock that a non-audio thread can hold, which is the
    /// one thing the callback must never do.
    pub fn new() -> Self {
        Self {
            detector: Detector::default_boxed(),
            policy: VadPolicy::default(),
        }
    }

    pub fn feed(&mut self, frame: &[f32]) -> f32 {
        let probability = self.detector.predict_f32(frame);
        self.policy.feed(probability);
        probability
    }

    /// Whether the take heard a confirmed onset. Read once, when the stream is
    /// torn down.
    pub fn speech_detected(&self) -> bool {
        self.policy.confirmed()
    }

    pub fn silence_ms(&self) -> u64 {
        self.policy.silence_ms()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEECH: f32 = 0.9;
    const SILENT: f32 = 0.0;
    const JUST_UNDER: f32 = 0.34;
    const JUST_OVER: f32 = 0.35;

    fn frames(policy: &mut VadPolicy, probability: f32, count: u64) {
        for _ in 0..count {
            policy.feed(probability);
        }
    }

    #[test]
    fn the_frame_length_is_the_earshot_frame_length() {
        // Not decoration: predict_f32 returns -1.0 for any other size, and the
        // whole policy is denominated in 16 ms.
        assert_eq!(FRAME_SAMPLES, 256);
        assert_eq!(FRAME_MS, 16);
        assert_eq!(FRAME_SAMPLES as u64 * FRAME_MS, 256 * 16);
    }

    #[test]
    fn a_single_frame_above_threshold_is_not_a_take() {
        let mut policy = VadPolicy::default();
        // A mouse click: one 16 ms frame, over the threshold. It is not a take,
        // and it is already counting as silence.
        policy.feed(SPEECH);
        assert!(!policy.confirmed(), "one frame must not confirm a take");
        assert_eq!(policy.silence_ms(), FRAME_MS);
    }

    #[test]
    fn speech_is_confirmed_at_the_fourth_frame_and_not_before() {
        // 60 ms is not a whole number of 16 ms frames, so the first frame at
        // which the onset completes is 64 ms.
        let mut policy = VadPolicy::default();
        frames(&mut policy, SPEECH, 3);
        assert!(!policy.confirmed(), "48 ms is three frames, not four");
        // A frame above threshold that has not completed the onset leaves the
        // silence clock at one frame: not zero, because it is not silence, and
        // not accumulating, so a stuttering onset cannot rack up an auto-stop.
        assert_eq!(policy.silence_ms(), FRAME_MS);
        policy.feed(SPEECH);
        assert!(policy.confirmed());
        assert_eq!(policy.silence_ms(), 0);
    }

    #[test]
    fn a_stutter_never_reaches_the_onset() {
        // Speech, speech, speech, silence, forever. The onset counter resets, so
        // nothing is ever confirmed. This is the case that keeps keyboard noise
        // from becoming a take.
        let mut policy = VadPolicy::default();
        for _ in 0..50 {
            frames(&mut policy, SPEECH, 3);
            policy.feed(SILENT);
        }
        assert!(!policy.confirmed());
    }

    #[test]
    fn the_threshold_is_inclusive_at_thirty_five_percent() {
        let mut policy = VadPolicy::default();
        frames(&mut policy, JUST_UNDER, 10);
        assert!(!policy.confirmed());
        let mut policy = VadPolicy::default();
        frames(&mut policy, JUST_OVER, 4);
        assert!(policy.confirmed());
    }

    #[test]
    fn the_hangover_holds_the_take_open_for_the_whole_pause() {
        let mut policy = VadPolicy::default();
        frames(&mut policy, SPEECH, 4);
        assert!(policy.confirmed());
        // 37 frames is 592 ms: still inside the 600 ms hangover, which is
        // observable as the silence clock not running.
        frames(&mut policy, SILENT, HANGOVER_FRAMES - 1);
        assert_eq!(policy.silence_ms(), 0, "the pause must not release the hangover");
        policy.feed(SILENT);
        assert_eq!(
            policy.silence_ms(),
            FRAME_MS,
            "38 frames is the release"
        );
    }

    #[test]
    fn silence_counts_from_the_end_of_the_hangover_not_the_last_word() {
        // The auto-stop is 1.6 s of silence, and the hangover is a separate
        // mechanism, so the real gap is 1.6 s + 608 ms. If this test ever fails,
        // the auto-stop has moved and 1.6 s no longer means 1.6 s.
        let mut policy = VadPolicy::default();
        frames(&mut policy, SPEECH, 4);
        let mut since_word = 0u64;
        let mut previous = policy.silence_ms();
        loop {
            policy.feed(SILENT);
            since_word += FRAME_MS;
            let silence = policy.silence_ms();
            // Silence advances by exactly one frame, or not at all while the
            // hangover is still running. It never jumps.
            assert!(
                silence == previous || silence == previous + FRAME_MS,
                "silence_ms jumped {previous} -> {silence}"
            );
            previous = silence;
            if silence > 1_600 {
                break;
            }
            assert!(since_word < 10_000, "the hangover never released");
        }
        assert_eq!(
            since_word,
            HANGOVER_MS + 1_600,
            "1.6 s of silence after a {HANGOVER_MS} ms hangover"
        );
        // The confirmed flag latches: a take that heard speech stays a take.
        assert!(policy.confirmed());
    }

    #[test]
    fn silence_resets_the_moment_speech_returns() {
        let mut policy = VadPolicy::default();
        frames(&mut policy, SPEECH, 4);
        frames(&mut policy, SILENT, 200);
        assert!(policy.silence_ms() > 1_600);
        // One returning frame is not silence, so the clock drops to a single
        // frame rather than to zero — and the onset has to complete again before
        // it clears outright, which is the same rule the first frame of a take
        // obeys.
        policy.feed(SPEECH);
        assert_eq!(policy.silence_ms(), FRAME_MS);
        frames(&mut policy, SPEECH, 3);
        assert!(policy.confirmed());
        assert_eq!(policy.silence_ms(), 0);
    }

    #[test]
    fn a_very_long_silence_does_not_overflow_the_clock() {
        let mut policy = VadPolicy::default();
        frames(&mut policy, SPEECH, 4);
        frames(&mut policy, SILENT, 75_000);
        assert!(policy.silence_ms() > 1_600);
        assert!(policy.confirmed(), "the flag latches for the whole take");
    }

    #[test]
    fn silence_counts_from_the_first_frame_of_an_empty_take() {
        // A take that hears nothing still has to reach the auto-stop, or it
        // would sit on "listening" for ever.
        let mut policy = VadPolicy::default();
        policy.feed(SILENT);
        assert_eq!(policy.silence_ms(), FRAME_MS);
        assert!(!policy.confirmed());
    }

    #[test]
    fn a_detector_answers_with_a_probability_in_range() {
        // earshot's real model on a real 256-sample frame: digital silence must
        // not read as speech, or the "Nothing heard" gate means nothing.
        let mut state = VadState::new();
        for _ in 0..8 {
            let probability = state.feed(&[0.0; FRAME_SAMPLES]);
            assert!(
                (0.0..=1.0).contains(&probability),
                "probability {probability} out of range"
            );
        }
        assert!(!state.speech_detected());
        assert!(state.silence_ms() >= 8 * FRAME_MS);
    }
}
