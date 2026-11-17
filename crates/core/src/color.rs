//! Color types: plain RGB, and the 5-channel pixel of a WS2805 strip.

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
