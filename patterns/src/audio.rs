//! Envelope-based audio features from raw electret-mic samples.
//!
//! Analog electret mics vary wildly in DC bias and gain, and the room noise
//! floor drifts, so the analyzer tracks a running DC offset and a running
//! loudness baseline rather than trusting fixed thresholds. Loudness is mean
//! absolute deviation (cheaper than true RMS, no sqrt) which is plenty for an
//! envelope.

/// Per-frame audio features handed to the simulation.
#[derive(Default, Clone, Copy)]
pub struct Audio {
    /// Loudness envelope, auto-gained to `0..=255`.
    pub level: u8,
    /// An onset (beat) was detected this frame.
    pub beat: bool,
}

// ponytail: these are starting points — tune on the real mic/room. Beat
// detection is the sensitive one; widen NOISE_GATE if it fires in silence,
// lower BEAT_RATIO if it misses soft beats.
/// Higher = slower DC-bias tracking.
const DC_TRACK: i32 = 16;
/// Higher = smoother loudness baseline.
const AVG_TRACK: u32 = 8;
/// Beat when `loudness * 2 > avg * BEAT_RATIO` (i.e. ~1.5× the baseline).
const BEAT_RATIO: u32 = 3;
/// Frames to suppress after a beat (~120 ms at a 20 ms frame).
const REFRACTORY: u8 = 6;
/// Absolute loudness floor (raw ADC counts) below which nothing counts as a beat.
const NOISE_GATE: u32 = 20;

/// Turns raw ADC samples into an auto-gained loudness envelope with onsets.
pub struct Envelope {
    dc: i32,
    avg: u32,
    refractory: u8,
}

impl Default for Envelope {
    fn default() -> Self {
        Envelope::new()
    }
}

impl Envelope {
    /// Start centred on a 12-bit ADC mid-scale; DC tracking corrects from there.
    pub const fn new() -> Self {
        Envelope {
            dc: 2048,
            avg: 0,
            refractory: 0,
        }
    }

    /// Process one frame's worth of samples and return its features.
    pub fn frame(&mut self, samples: &[u16]) -> Audio {
        if samples.is_empty() {
            self.refractory = self.refractory.saturating_sub(1);
            return Audio::default();
        }

        let n = samples.len();
        let mean = (samples.iter().map(|&s| s as i64).sum::<i64>() / n as i64) as i32;
        self.dc += (mean - self.dc) / DC_TRACK;

        let acc: u64 = samples
            .iter()
            .map(|&s| (s as i32 - self.dc).unsigned_abs() as u64)
            .sum();
        let loudness = (acc / n as u64) as u32;

        let beat =
            self.refractory == 0 && loudness > NOISE_GATE && loudness * 2 > self.avg * BEAT_RATIO;
        if beat {
            self.refractory = REFRACTORY;
        } else {
            self.refractory = self.refractory.saturating_sub(1);
        }

        // Track the loudness baseline (used for both auto-gain and beat ratio).
        if loudness > self.avg {
            self.avg += (loudness - self.avg) / AVG_TRACK;
        } else {
            self.avg -= (self.avg - loudness) / AVG_TRACK;
        }

        // Gate the noise floor first so true silence reads as 0, then auto-gain
        // what's left relative to the running baseline.
        let audible = loudness.saturating_sub(NOISE_GATE);
        let reference = self.avg * 2 + 1;
        let level = (audible * 255 / reference).min(255) as u8;
        Audio { level, beat }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One frame of a tone: `n` samples oscillating `±amp` around a DC bias.
    fn tone(dc: u16, amp: u16, n: usize) -> Vec<u16> {
        (0..n)
            .map(|i| if i % 2 == 0 { dc + amp } else { dc - amp })
            .collect()
    }

    #[test]
    fn silence_is_quiet_and_beatless() {
        let mut env = Envelope::new();
        let quiet = tone(2048, 1, 64);
        let mut any_beat = false;
        let mut last = 0;
        for _ in 0..50 {
            let a = env.frame(&quiet);
            any_beat |= a.beat;
            last = a.level;
        }
        assert!(!any_beat, "silence should not trigger beats");
        assert!(last < 40, "silence should read as low level, got {last}");
    }

    #[test]
    fn loud_transient_after_quiet_triggers_exactly_one_beat() {
        let mut env = Envelope::new();
        let quiet = tone(2048, 2, 64);
        for _ in 0..30 {
            env.frame(&quiet);
        }
        let hit = tone(2048, 800, 64);
        let first = env.frame(&hit);
        let second = env.frame(&hit);
        assert!(first.beat, "a jump above the baseline should fire a beat");
        assert!(
            !second.beat,
            "the same sustained level should not re-fire (refractory)"
        );
    }

    #[test]
    fn auto_gain_settles_a_sustained_tone_toward_mid_scale() {
        let mut env = Envelope::new();
        let steady = tone(2048, 400, 64);
        let mut level = 0;
        for _ in 0..100 {
            level = env.frame(&steady).level;
        }
        // A constant amplitude should not stay pinned at full scale.
        assert!(
            (80..=180).contains(&level),
            "auto-gain should centre level, got {level}"
        );
    }

    #[test]
    fn dc_bias_is_rejected() {
        let mut env = Envelope::new();
        let biased_flat = tone(3200, 0, 64); // strong DC offset, no signal
        let mut level = 255;
        for _ in 0..300 {
            level = env.frame(&biased_flat).level;
        }
        assert!(
            level < 20,
            "a flat DC-biased signal should read as quiet, got {level}"
        );
    }
}
