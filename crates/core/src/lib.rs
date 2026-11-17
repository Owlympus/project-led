//! Hardware-independent lighting logic: colors, white temperature, and
//! (later) transitions and effects.
//!
//! This crate is `no_std` so it can run on the ESP32-C6, but its tests run
//! on the host machine with a plain `cargo test -p led_core`.

// Use the standard library only when compiling tests on the host.
#![cfg_attr(not(test), no_std)]

pub mod color;
pub mod white;

// Re-exports so users can write `led_core::Rgb` instead of `led_core::color::Rgb`.
pub use color::{Rgb, Rgbcct};
pub use white::{ColorTemperature, WhiteChannels, WhiteMix};
