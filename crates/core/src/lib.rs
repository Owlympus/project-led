//! Hardware-independent lighting logic: colors (RGB, HSV), white temperature,
//! gamma correction and transitions (effects will come later).
//!
//! This crate is `no_std` so it can run on the ESP32-C6, but its tests run
//! on the host machine with a plain `cargo test -p led_core`.

// Use the standard library only when compiling tests on the host.
#![cfg_attr(not(test), no_std)]

pub mod color;
pub mod gamma;
pub mod hsv;
pub mod transition;
pub mod white;

// Re-exports so users can write `led_core::Rgb` instead of `led_core::color::Rgb`.
pub use color::{Rgb, Rgbcct};
pub use gamma::gamma8;
pub use hsv::Hsv;
pub use transition::{Easing, Transition};
pub use white::{ColorTemperature, WhiteChannels, WhiteMix};
