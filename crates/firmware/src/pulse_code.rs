//! Turning `ws2805` pulses (in nanoseconds) into RMT pulse codes (in ticks).
//!
//! Shared by the on-board LED ([`crate::board_led`]) and the strip
//! ([`crate::strip`]).

use esp_hal::gpio::Level;
use esp_hal::rmt::PulseCode;
use ws2805::Pulse;

/// A pulse as an RMT pulse code: high, then low.
pub fn to_code(pulse: Pulse, tick_hz: u32) -> PulseCode {
    let (high, low) = pulse.to_ticks(tick_hz);
    PulseCode::new(Level::High, high, Level::Low, low)
}

/// Writes `pulses` into `codes`, followed by an end marker telling the RMT to
/// stop. Returns how many codes were used (end marker included).
///
/// Panics if `codes` is too short: that is a sizing bug in the caller.
pub fn fill(codes: &mut [PulseCode], pulses: impl Iterator<Item = Pulse>, tick_hz: u32) -> usize {
    let mut used = 0;
    for pulse in pulses {
        codes[used] = to_code(pulse, tick_hz);
        used += 1;
    }
    codes[used] = PulseCode::end_marker();
    used + 1
}
