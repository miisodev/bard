export type AudioCue = "capture" | "copy" | "dictate-start" | "dictate-stop" | "error";

let context: AudioContext | null = null;

const tones: Record<AudioCue, { from: number; to: number; duration: number; gain: number }> = {
  capture: { from: 1500, to: 480, duration: 0.07, gain: 0.16 },
  copy: { from: 880, to: 660, duration: 0.045, gain: 0.08 },
  "dictate-start": { from: 420, to: 620, duration: 0.09, gain: 0.08 },
  "dictate-stop": { from: 620, to: 360, duration: 0.09, gain: 0.08 },
  error: { from: 260, to: 190, duration: 0.16, gain: 0.1 },
};

export function playCue(cue: AudioCue) {
  try {
    context ??= new AudioContext();
    const ctx = context;
    if (ctx.state === "suspended") void ctx.resume();
    const tone = tones[cue];
    const oscillator = ctx.createOscillator();
    const gain = ctx.createGain();
    const now = ctx.currentTime;
    oscillator.type = "sine";
    oscillator.frequency.setValueAtTime(tone.from, now);
    oscillator.frequency.exponentialRampToValueAtTime(tone.to, now + tone.duration);
    gain.gain.setValueAtTime(0.0001, now);
    gain.gain.exponentialRampToValueAtTime(tone.gain, now + 0.008);
    gain.gain.exponentialRampToValueAtTime(0.0001, now + tone.duration);
    oscillator.connect(gain);
    gain.connect(ctx.destination);
    oscillator.start(now);
    oscillator.stop(now + tone.duration + 0.01);
  } catch {
    // Audio feedback is deliberately best-effort and never blocks an action.
  }
}
