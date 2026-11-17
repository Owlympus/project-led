//! How long each bit stays high and low on the data wire.
//!
//! Durations are in nanoseconds here. The firmware will convert them to
//! ticks of the RMT peripheral's clock.

/// One bit on the wire: the line stays high for `high_ns`, then low for `low_ns`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pulse {
    pub high_ns: u16,
    pub low_ns: u16,
}

impl Pulse {
    pub const fn new(high_ns: u16, low_ns: u16) -> Self {
        Self { high_ns, low_ns }
    }

    /// Total duration of the bit.
    pub const fn period_ns(&self) -> u32 {
        self.high_ns as u32 + self.low_ns as u32
    }

    /// The pulse as `(high, low)` counts of a clock ticking `tick_hz` times
    /// per second, rounded to the nearest tick. That is what the RMT peripheral
    /// expects (at 80 MHz, one tick = 12.5 ns).
    pub const fn to_ticks(&self, tick_hz: u32) -> (u16, u16) {
        (
            ns_to_ticks(self.high_ns, tick_hz),
            ns_to_ticks(self.low_ns, tick_hz),
        )
    }
}

/// `ns × tick_hz / 1 000 000 000`, rounded. Computed in `u64` because
/// `ns × tick_hz` easily exceeds `u32` (1000 × 80 000 000).
const fn ns_to_ticks(ns: u16, tick_hz: u32) -> u16 {
    let ticks = (ns as u64 * tick_hz as u64 + 500_000_000) / 1_000_000_000;
    if ticks > u16::MAX as u64 {
        u16::MAX
    } else {
        ticks as u16
    }
}

/// The timings of one LED chip model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timing {
    /// How a 0 bit is sent.
    pub zero: Pulse,
    /// How a 1 bit is sent.
    pub one: Pulse,
    /// How long the line stays low after the last pixel so the strip shows the data.
    pub reset_us: u16,
}

impl Timing {
    /// WS2805 (our strip). Datasheet: T0H 220–380 ns, T1H 580–1000 ns,
    /// T0L and T1L 580–1000 ns, period ≥ 1250 ns, reset > 280 µs.
    /// Our values sit inside every range, and keep both bit periods ≥ 1250 ns.
    pub const WS2805: Timing = Timing {
        zero: Pulse::new(350, 950),
        one: Pulse::new(800, 600),
        reset_us: 300,
    };

    /// WS2812B (the RGB LED on the dev board). Datasheet: T0H 400 ± 150 ns,
    /// T0L 850 ± 150 ns, T1H 800 ± 150 ns, T1L 450 ± 150 ns. Recent versions
    /// need a reset of at least 280 µs (older ones: 50 µs).
    pub const WS2812: Timing = Timing {
        zero: Pulse::new(350, 850),
        one: Pulse::new(800, 450),
        reset_us: 300,
    };

    /// The pulse that sends `bit`.
    pub const fn pulse(&self, bit: bool) -> Pulse {
        if bit { self.one } else { self.zero }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ws2805_is_within_the_datasheet() {
        let t = Timing::WS2805;
        assert!((220..=380).contains(&t.zero.high_ns));
        assert!((580..=1000).contains(&t.one.high_ns));
        assert!((580..=1000).contains(&t.zero.low_ns));
        assert!((580..=1000).contains(&t.one.low_ns));
        assert!(t.zero.period_ns() >= 1250);
        assert!(t.one.period_ns() >= 1250);
        assert!(t.reset_us > 280);
    }

    #[test]
    fn ws2812_is_within_the_datasheet() {
        let t = Timing::WS2812;
        assert!((250..=550).contains(&t.zero.high_ns));
        assert!((700..=1000).contains(&t.zero.low_ns));
        assert!((650..=950).contains(&t.one.high_ns));
        assert!((300..=600).contains(&t.one.low_ns));
        assert!(t.reset_us >= 280);
    }

    #[test]
    fn ticks_at_80_mhz() {
        // 80 MHz: 1 tick = 12.5 ns.
        assert_eq!(Pulse::new(350, 850).to_ticks(80_000_000), (28, 68));
        assert_eq!(Pulse::new(800, 600).to_ticks(80_000_000), (64, 48));
    }

    #[test]
    fn ticks_are_rounded() {
        // 10 MHz: 1 tick = 100 ns. 349 ns → 3.49 → 3 ; 350 ns → 3.5 → 4.
        assert_eq!(Pulse::new(349, 350).to_ticks(10_000_000), (3, 4));
    }

    #[test]
    fn pulse_picks_zero_or_one() {
        let t = Timing::WS2805;
        assert_eq!(t.pulse(false), t.zero);
        assert_eq!(t.pulse(true), t.one);
    }
}
