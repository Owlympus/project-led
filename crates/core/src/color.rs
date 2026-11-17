//! Color types: plain RGB, and the 5-channel pixel of a WS2805 strip.

use crate::gamma::gamma8;
use crate::white::WhiteMix;

/// A color made of red, green and blue, each from 0 (off) to 255 (full).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const BLACK: Rgb = Rgb::new(0, 0, 0);
    pub const WHITE: Rgb = Rgb::new(255, 255, 255);

    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Returns this color dimmed to `brightness` (0 = off, 255 = unchanged).
    pub fn scale(self, brightness: u8) -> Self {
        Self {
            r: scale8(self.r, brightness),
            g: scale8(self.g, brightness),
            b: scale8(self.b, brightness),
        }
    }

    /// Returns this color with gamma correction applied to every channel.
    /// Apply it last, right before sending to the strip (see [`crate::gamma`]).
    pub fn gamma_corrected(self) -> Self {
        Self::new(gamma8(self.r), gamma8(self.g), gamma8(self.b))
    }
}

/// One pixel of a WS2805 strip: red, green, blue, warm white and cold white.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rgbcct {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub warm: u8,
    pub cold: u8,
}

impl Rgbcct {
    pub const OFF: Rgbcct = Rgbcct::new(0, 0, 0, 0, 0);

    pub const fn new(r: u8, g: u8, b: u8, warm: u8, cold: u8) -> Self {
        Self {
            r,
            g,
            b,
            warm,
            cold,
        }
    }

    /// A colored pixel with both white channels off.
    pub const fn from_rgb(rgb: Rgb) -> Self {
        Self::new(rgb.r, rgb.g, rgb.b, 0, 0)
    }

    /// A white pixel with the RGB channels off.
    pub const fn from_white(mix: WhiteMix) -> Self {
        Self::new(0, 0, 0, mix.warm, mix.cold)
    }

    /// Returns this pixel dimmed to `brightness` (0 = off, 255 = unchanged).
    pub fn scale(self, brightness: u8) -> Self {
        Self {
            r: scale8(self.r, brightness),
            g: scale8(self.g, brightness),
            b: scale8(self.b, brightness),
            warm: scale8(self.warm, brightness),
            cold: scale8(self.cold, brightness),
        }
    }

    /// Returns this pixel with gamma correction applied to every channel.
    /// Apply it last, right before sending to the strip (see [`crate::gamma`]).
    pub fn gamma_corrected(self) -> Self {
        Self::new(
            gamma8(self.r),
            gamma8(self.g),
            gamma8(self.b),
            gamma8(self.warm),
            gamma8(self.cold),
        )
    }

    /// Mixes this pixel with `other`: `t = 0` gives `self`, `t = 65535`
    /// gives `other`, in between a blend of both (channel by channel).
    /// Used by transitions (over time) and gradients (along the strip).
    pub fn lerp(self, other: Rgbcct, t: u16) -> Self {
        Self::new(
            lerp8(self.r, other.r, t),
            lerp8(self.g, other.g, t),
            lerp8(self.b, other.b, t),
            lerp8(self.warm, other.warm, t),
            lerp8(self.cold, other.cold, t),
        )
    }
}

/// Linear interpolation between `a` (t = 0) and `b` (t = 65535), rounded.
fn lerp8(a: u8, b: u8, t: u16) -> u8 {
    const MAX: u32 = u16::MAX as u32;
    let t = t as u32;
    let mixed = a as u32 * (MAX - t) + b as u32 * t;
    ((mixed + MAX / 2) / MAX) as u8
}

/// Computes `value * brightness / 255`, rounded to the nearest integer.
///
/// The multiplication is done in `u16` because 255 * 255 does not fit in a `u8`.
pub(crate) fn scale8(value: u8, brightness: u8) -> u8 {
    let product = value as u16 * brightness as u16;
    ((product + 127) / 255) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lerp8_endpoints_and_middle() {
        assert_eq!(lerp8(0, 200, 0), 0);
        assert_eq!(lerp8(0, 200, u16::MAX), 200);
        assert_eq!(lerp8(0, 200, u16::MAX / 2), 100);
        assert_eq!(lerp8(200, 0, u16::MAX / 2), 100);
        assert_eq!(lerp8(42, 42, 12_345), 42);
    }

    #[test]
    fn lerp_mixes_every_channel() {
        let a = Rgbcct::new(0, 100, 200, 0, 50);
        let b = Rgbcct::new(200, 100, 0, 100, 50);
        assert_eq!(a.lerp(b, 0), a);
        assert_eq!(a.lerp(b, u16::MAX), b);
        assert_eq!(a.lerp(b, u16::MAX / 2), Rgbcct::new(100, 100, 100, 50, 50));
    }

    #[test]
    fn scale8_keeps_extremes() {
        assert_eq!(scale8(255, 255), 255);
        assert_eq!(scale8(200, 255), 200);
        assert_eq!(scale8(200, 0), 0);
        assert_eq!(scale8(0, 255), 0);
    }

    #[test]
    fn scale8_halves() {
        assert_eq!(scale8(255, 128), 128);
        assert_eq!(scale8(100, 128), 50);
    }

    #[test]
    fn rgb_scale_dims_every_channel() {
        let orange = Rgb::new(255, 128, 0);
        assert_eq!(orange.scale(128), Rgb::new(128, 64, 0));
    }

    #[test]
    fn gamma_corrected_applies_to_every_channel() {
        assert_eq!(
            Rgb::new(0, 128, 255).gamma_corrected(),
            Rgb::new(0, 56, 255)
        );
        assert_eq!(
            Rgbcct::new(255, 128, 0, 128, 255).gamma_corrected(),
            Rgbcct::new(255, 56, 0, 56, 255)
        );
    }

    #[test]
    fn rgbcct_from_rgb_turns_whites_off() {
        let pixel = Rgbcct::from_rgb(Rgb::new(1, 2, 3));
        assert_eq!(pixel, Rgbcct::new(1, 2, 3, 0, 0));
    }

    #[test]
    fn rgbcct_from_white_turns_rgb_off() {
        let pixel = Rgbcct::from_white(WhiteMix { warm: 10, cold: 20 });
        assert_eq!(pixel, Rgbcct::new(0, 0, 0, 10, 20));
    }
}
