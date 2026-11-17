//! Ambiances: ready-made scenes drawn pixel by pixel along the bar (the
//! "pixels and ambiances" idea box in the ROADMAP).
//!
//! Each ambiance fills a frame at full brightness, from the time elapsed
//! since it started. The caller then applies the lamp's brightness (the Home
//! app's slider) and, last, gamma correction.

use crate::color::Rgbcct;
use crate::effect::{gradient_at, ping_pong, pixel_position, smooth_noise};

/// The ambiances the lamp knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ambiance {
    /// A still gradient: orange at one end, through pink, to violet.
    Sunset,
    /// Green, cyan and violet, slowly drifting along the bar.
    Aurora,
    /// Warm oranges and whites, flickering gently like flames.
    Campfire,
}

/// Orange (with some warm white for depth) → pink → violet.
const SUNSET: [Rgbcct; 3] = [
    Rgbcct::new(255, 90, 0, 80, 0),
    Rgbcct::new(255, 30, 90, 0, 0),
    Rgbcct::new(110, 0, 170, 0, 0),
];

/// Green → cyan → violet.
const AURORA: [Rgbcct; 3] = [
    Rgbcct::new(0, 255, 60, 0, 0),
    Rgbcct::new(0, 180, 200, 0, 0),
    Rgbcct::new(120, 0, 255, 0, 0),
];
/// The aurora drifts by one full bar length every 20 s (one way), then back.
const AURORA_DRIFT_MS: u32 = 20_000;

/// Deep orange → amber → warm white: the colors of a flame, from the
/// embers to the brightest part.
const CAMPFIRE: [Rgbcct; 3] = [
    Rgbcct::new(255, 50, 0, 0, 0),
    Rgbcct::new(255, 110, 0, 120, 0),
    Rgbcct::new(60, 20, 0, 255, 0),
];
/// How often each flame picks a new target (color and brightness).
const FLICKER_STEP_MS: u32 = 140;
/// The flames never go below this brightness (~60 %), so the bar never
/// looks like it is going out.
const FLICKER_MIN: u8 = 150;

impl Ambiance {
    /// Every ambiance, in a fixed order (to list them in Matter).
    pub const ALL: [Ambiance; 3] = [Ambiance::Sunset, Ambiance::Aurora, Ambiance::Campfire];

    /// The name shown to the user (logs, Home app).
    pub const fn name(self) -> &'static str {
        match self {
            Ambiance::Sunset => "Coucher de soleil",
            Ambiance::Aurora => "Aurore boréale",
            Ambiance::Campfire => "Feu de camp",
        }
    }

    /// Fills `pixels` with the frame at `elapsed_ms`, at full brightness.
    pub fn render(self, elapsed_ms: u32, pixels: &mut [Rgbcct]) {
        let len = pixels.len();
        for (index, pixel) in pixels.iter_mut().enumerate() {
            let position = pixel_position(index, len);
            *pixel = match self {
                Ambiance::Sunset => gradient_at(&SUNSET, position),
                Ambiance::Aurora => {
                    // Shift the gradient over time; `ping_pong` folds the
                    // shifted position back into the bar without a jump.
                    let drift =
                        (elapsed_ms as u64 * u16::MAX as u64 / AURORA_DRIFT_MS as u64) as u32;
                    gradient_at(&AURORA, ping_pong(position as u32 + drift))
                }
                Ambiance::Campfire => {
                    // Two independent drifts per pixel: one picks the color
                    // in the flame's palette, the other its brightness.
                    let seed = index as u32 * 2;
                    let heat = smooth_noise(seed, elapsed_ms, FLICKER_STEP_MS);
                    let flicker = smooth_noise(seed + 1, elapsed_ms, FLICKER_STEP_MS);
                    let level =
                        FLICKER_MIN as u32 + flicker as u32 * (255 - FLICKER_MIN as u32) / 255;
                    gradient_at(&CAMPFIRE, heat as u16 * 257).scale(level as u8)
                }
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(ambiance: Ambiance, elapsed_ms: u32) -> [Rgbcct; 30] {
        let mut pixels = [Rgbcct::OFF; 30];
        ambiance.render(elapsed_ms, &mut pixels);
        pixels
    }

    #[test]
    fn sunset_goes_from_orange_to_violet_and_stays_still() {
        let pixels = frame(Ambiance::Sunset, 0);
        assert_eq!(pixels[0], SUNSET[0]);
        assert_eq!(pixels[29], SUNSET[2]);
        assert_eq!(frame(Ambiance::Sunset, 12_345), pixels, "no animation");
    }

    #[test]
    fn aurora_drifts_and_comes_back() {
        let start = frame(Ambiance::Aurora, 0);
        assert_eq!(start[0], AURORA[0], "starts as the plain gradient");
        assert_ne!(frame(Ambiance::Aurora, 5_000), start, "it moves");
        // There and back: the same picture after two drifts.
        assert_eq!(frame(Ambiance::Aurora, 2 * AURORA_DRIFT_MS), start);
    }

    #[test]
    fn campfire_flickers_but_never_goes_out() {
        let a = frame(Ambiance::Campfire, 0);
        let b = frame(Ambiance::Campfire, 1_000);
        assert_ne!(a, b, "it flickers");
        for elapsed_ms in (0..5_000).step_by(37) {
            for pixel in frame(Ambiance::Campfire, elapsed_ms) {
                // Along the palette, the strongest of red and warm never
                // drops below ~175 (the dip between amber and warm white),
                // and the flicker keeps at least ~60 % of it: ≥ ~100.
                assert!(pixel.r.max(pixel.warm) >= 100, "{elapsed_ms} ms: {pixel:?}");
            }
        }
    }

    #[test]
    fn campfire_pixels_flicker_independently() {
        let pixels = frame(Ambiance::Campfire, 2_000);
        assert!(pixels.iter().any(|p| *p != pixels[0]), "not all the same");
    }

    #[test]
    fn every_ambiance_has_a_name() {
        for ambiance in Ambiance::ALL {
            assert!(!ambiance.name().is_empty());
        }
    }
}
