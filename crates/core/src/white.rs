//! White color temperature, and how to mix the strip's warm and cold whites
//! to reach a target temperature.
//!
//! Temperatures are stored in **mireds** (micro reciprocal degrees):
//! `mireds = 1_000_000 / kelvin`. Matter (and Hue) use mireds, and mixing two
//! white LEDs linearly in mireds looks more even to the eye than in kelvin.
//! Note: a *higher* mired value means a *warmer* (more orange) white.

/// A white color temperature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorTemperature {
    mireds: u16,
}

impl ColorTemperature {
    /// Creates a temperature from mireds (e.g. 370 ≈ 2700 K warm white).
    pub const fn from_mireds(mireds: u16) -> Self {
        Self { mireds }
    }

    /// Creates a temperature from kelvin (e.g. 2700 = warm, 6500 = cold).
    ///
    /// The result is rounded to the nearest mired, so converting back with
    /// [`kelvin`](Self::kelvin) may differ by a few kelvin.
    pub const fn from_kelvin(kelvin: u16) -> Self {
        Self::from_mireds(reciprocal(kelvin))
    }

    pub const fn mireds(self) -> u16 {
        self.mireds
    }

    pub const fn kelvin(self) -> u16 {
        reciprocal(self.mireds)
    }
}

/// Computes `1_000_000 / x` rounded to the nearest integer, saturating at
/// `u16::MAX`. A value of 0 is treated as 1 to avoid dividing by zero.
const fn reciprocal(x: u16) -> u16 {
    let x = if x == 0 { 1 } else { x as u32 };
    let result = (1_000_000 + x / 2) / x;
    if result > u16::MAX as u32 {
        u16::MAX
    } else {
        result as u16
    }
}

/// The two white LEDs of a strip, described by their own temperatures.
/// Read them from the strip's datasheet (typically ~2700 K and ~6500 K).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WhiteChannels {
    pub warm: ColorTemperature,
    pub cold: ColorTemperature,
}

/// How much of each white LED to light, from 0 to 255.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WhiteMix {
    pub warm: u8,
    pub cold: u8,
}

impl WhiteChannels {
    pub const fn new(warm: ColorTemperature, cold: ColorTemperature) -> Self {
        Self { warm, cold }
    }

    /// Returns the warm/cold mix that best reproduces `target`, at full
    /// brightness (`warm + cold == 255`).
    ///
    /// Targets warmer than the warm LED give pure warm; targets colder than
    /// the cold LED give pure cold.
    pub fn mix(&self, target: ColorTemperature) -> WhiteMix {
        let warmest = self.warm.mireds() as u32;
        let coldest = self.cold.mireds() as u32;

        // Misconfigured channels (warm not warmer than cold): use warm only.
        if warmest <= coldest {
            return WhiteMix { warm: 255, cold: 0 };
        }

        let target = (target.mireds() as u32).clamp(coldest, warmest);
        let span = warmest - coldest;

        // 0 when target is the warm LED, 255 when it is the cold LED.
        let cold = ((warmest - target) * 255 + span / 2) / span;
        let cold = cold as u8;

        WhiteMix {
            warm: 255 - cold,
            cold,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A typical strip: 2700 K warm LED, 6500 K cold LED.
    const STRIP: WhiteChannels = WhiteChannels::new(
        ColorTemperature::from_kelvin(2700),
        ColorTemperature::from_kelvin(6500),
    );

    #[test]
    fn kelvin_to_mireds() {
        assert_eq!(ColorTemperature::from_kelvin(2700).mireds(), 370);
        assert_eq!(ColorTemperature::from_kelvin(6500).mireds(), 154);
    }

    #[test]
    fn kelvin_round_trip_is_close() {
        let kelvin = ColorTemperature::from_kelvin(2700).kelvin();
        assert!(kelvin.abs_diff(2700) <= 10, "got {kelvin} K");
    }

    #[test]
    fn zero_does_not_panic() {
        assert_eq!(ColorTemperature::from_kelvin(0).mireds(), u16::MAX);
        assert_eq!(ColorTemperature::from_mireds(0).kelvin(), u16::MAX);
    }

    #[test]
    fn mix_at_the_led_temperatures_is_pure() {
        assert_eq!(STRIP.mix(STRIP.warm), WhiteMix { warm: 255, cold: 0 });
        assert_eq!(STRIP.mix(STRIP.cold), WhiteMix { warm: 0, cold: 255 });
    }

    #[test]
    fn mix_out_of_range_is_clamped() {
        let candle = ColorTemperature::from_kelvin(1800);
        let sky = ColorTemperature::from_kelvin(10_000);
        assert_eq!(STRIP.mix(candle), WhiteMix { warm: 255, cold: 0 });
        assert_eq!(STRIP.mix(sky), WhiteMix { warm: 0, cold: 255 });
    }

    #[test]
    fn mix_in_the_middle_is_about_half() {
        // Halfway between 370 and 154 mireds.
        let middle = ColorTemperature::from_mireds(262);
        let mix = STRIP.mix(middle);
        assert_eq!(mix.warm as u16 + mix.cold as u16, 255);
        assert!(mix.warm.abs_diff(mix.cold) <= 1, "got {mix:?}");
    }

    #[test]
    fn misconfigured_channels_fall_back_to_warm() {
        let reversed = WhiteChannels::new(STRIP.cold, STRIP.warm);
        assert_eq!(
            reversed.mix(ColorTemperature::from_kelvin(4000)),
            WhiteMix { warm: 255, cold: 0 }
        );
    }
}
