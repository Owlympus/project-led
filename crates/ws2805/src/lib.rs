//! Driver for WS2805 LED strips (red, green, blue, warm white, cold white).
//!
//! The strip uses a single data wire. Each pixel receives 5 bytes (40 bits),
//! and every bit is sent as a high pulse followed by a low pulse: a short high
//! pulse means 0, a long one means 1. A long low level ("reset") then tells
//! every pixel to show what it received.
//!
//! This crate is split in two layers:
//! - [`encode`] and [`timing`]: pixel → bytes → bits → pulses. Pure logic,
//!   tested on the Mac with `cargo test -p ws2805`;
//! - *(to come)* sending the pulses with the ESP32-C6 RMT peripheral.
//!
//! Pixels must already be gamma corrected (see `led_core::gamma`): this crate
//! sends values as they are.

#![cfg_attr(not(test), no_std)]

pub mod encode;
pub mod timing;

pub use encode::{ColorOrder, WhiteOrder, bits, pulses, strip_bytes, ws2805_bytes, ws2812_bytes};
pub use timing::{Pulse, Timing};
