//! Smooth transitions from one pixel color to another over time.
//!
//! This module does not read any clock: the caller passes the elapsed time.
//! That keeps it independent from the hardware and easy to test. On the
//! ESP32-C6, the firmware will call [`Transition::at`] on every frame with the
//! time since the transition started.
//!
//! Transitions work on colors *before* gamma correction:
//! target color → transition → gamma → strip.

use crate::color::Rgbcct;

/// Progress through a transition: 0 = start, `PROGRESS_MAX` = end.
type Progress = u16;
const PROGRESS_MAX: u32 = Progress::MAX as u32;

/// The "shape" of a transition over time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Easing {
    /// Constant speed. Can look abrupt at the start and the end.
    Linear,
    /// Starts slowly, speeds up, then slows down before the end ("smoothstep").
    /// This is the default because it looks the most natural on a lamp.
    #[default]
    EaseInOut,
}

impl Easing {
    /// Maps linear time progress to eased progress (both `0..=65535`).
    fn apply(self, progress: Progress) -> Progress {
        match self {
            Easing::Linear => progress,
            Easing::EaseInOut => {
                // smoothstep: p² × (3 − 2p), with p scaled to 0..=65535.
                // u64 because the intermediate products exceed u32.
                let p = progress as u64;
                let max = PROGRESS_MAX as u64 + 1; // 65536 = 1.0
                let p_squared = p * p / max;
                (p_squared * (3 * max - 2 * p) / max) as Progress
            }
        }
    }
}

/// A transition from one pixel color to another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Transition {
    from: Rgbcct,
    to: Rgbcct,
    duration_ms: u32,
    easing: Easing,
}

impl Transition {
    /// A transition of `duration_ms` milliseconds, with the default easing.
    pub fn new(from: Rgbcct, to: Rgbcct, duration_ms: u32) -> Self {
        Self {
            from,
            to,
            duration_ms,
            easing: Easing::default(),
        }
    }

    /// No transition: the color is `to` immediately.
    pub fn instant(to: Rgbcct) -> Self {
        Self::new(to, to, 0)
    }

    /// Returns the same transition with another easing.
    pub fn with_easing(self, easing: Easing) -> Self {
        Self { easing, ..self }
    }

    /// The color the transition ends on.
    pub fn target(&self) -> Rgbcct {
        self.to
    }

    /// Whether the transition has reached its target after `elapsed_ms`.
    pub fn is_done(&self, elapsed_ms: u32) -> bool {
        elapsed_ms >= self.duration_ms
    }

    /// The color to display `elapsed_ms` milliseconds after the start.
    pub fn at(&self, elapsed_ms: u32) -> Rgbcct {
        if self.is_done(elapsed_ms) {
            return self.to;
        }
        let linear =
            (elapsed_ms as u64 * PROGRESS_MAX as u64 / self.duration_ms as u64) as Progress;
        let t = self.easing.apply(linear);

        Rgbcct::new(
            lerp8(self.from.r, self.to.r, t),
            lerp8(self.from.g, self.to.g, t),
            lerp8(self.from.b, self.to.b, t),
            lerp8(self.from.warm, self.to.warm, t),
            lerp8(self.from.cold, self.to.cold, t),
        )
    }
}

/// Linear interpolation between `a` (t = 0) and `b` (t = 65535), rounded.
fn lerp8(a: u8, b: u8, t: Progress) -> u8 {
    let t = t as u32;
    let mixed = a as u32 * (PROGRESS_MAX - t) + b as u32 * t;
    ((mixed + PROGRESS_MAX / 2) / PROGRESS_MAX) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLACK: Rgbcct = Rgbcct::OFF;
    const WARM: Rgbcct = Rgbcct::new(0, 0, 0, 200, 0);
    const RED: Rgbcct = Rgbcct::new(255, 0, 0, 0, 0);

    #[test]
    fn lerp8_endpoints_and_middle() {
        assert_eq!(lerp8(0, 200, 0), 0);
        assert_eq!(lerp8(0, 200, u16::MAX), 200);
        assert_eq!(lerp8(0, 200, u16::MAX / 2), 100);
        assert_eq!(lerp8(200, 0, u16::MAX / 2), 100);
        assert_eq!(lerp8(42, 42, 12_345), 42);
    }

    #[test]
    fn easing_endpoints_are_kept() {
        for easing in [Easing::Linear, Easing::EaseInOut] {
            assert_eq!(easing.apply(0), 0, "{easing:?}");
            assert_eq!(easing.apply(u16::MAX), u16::MAX, "{easing:?}");
        }
    }

    #[test]
    fn ease_in_out_is_slower_at_the_start_and_symmetric() {
        let quarter = u16::MAX / 4;
        assert!(Easing::EaseInOut.apply(quarter) < quarter);
        let half = Easing::EaseInOut.apply(32_768);
        assert!(half.abs_diff(32_768) <= 1, "got {half}");
    }

    #[test]
    fn ease_in_out_never_goes_backwards() {
        let mut previous = 0;
        for p in (0..=u16::MAX).step_by(97) {
            let eased = Easing::EaseInOut.apply(p);
            assert!(eased >= previous, "at {p}");
            previous = eased;
        }
    }

    #[test]
    fn starts_on_from_and_ends_on_to() {
        let fade = Transition::new(BLACK, WARM, 400);
        assert_eq!(fade.at(0), BLACK);
        assert_eq!(fade.at(400), WARM);
        assert_eq!(fade.at(10_000), WARM, "stays on target afterwards");
    }

    #[test]
    fn linear_middle_is_half_way() {
        let fade = Transition::new(BLACK, WARM, 400).with_easing(Easing::Linear);
        assert_eq!(fade.at(200), Rgbcct::new(0, 0, 0, 100, 0));
    }

    #[test]
    fn eased_fade_starts_slower_than_linear() {
        let linear = Transition::new(BLACK, WARM, 400).with_easing(Easing::Linear);
        let eased = Transition::new(BLACK, WARM, 400);
        assert!(eased.at(100).warm < linear.at(100).warm);
    }

    #[test]
    fn instant_and_zero_duration_jump_to_target() {
        assert_eq!(Transition::instant(RED).at(0), RED);
        assert_eq!(Transition::new(BLACK, RED, 0).at(0), RED);
        assert!(Transition::instant(RED).is_done(0));
    }

    #[test]
    fn is_done_only_at_the_end() {
        let fade = Transition::new(BLACK, RED, 400);
        assert!(!fade.is_done(399));
        assert!(fade.is_done(400));
    }

    #[test]
    fn changing_target_mid_way_continues_from_current_color() {
        // A new command arrives half-way through a fade to warm white:
        // the new transition must start from what is displayed, not jump.
        let first = Transition::new(BLACK, WARM, 400);
        let shown = first.at(200);
        let second = Transition::new(shown, RED, 400);
        assert_eq!(second.at(0), shown);
        assert_eq!(second.at(400), RED);
        assert_eq!(second.target(), RED);
    }
}
