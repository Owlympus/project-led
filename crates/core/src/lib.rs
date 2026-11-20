//! Hardware-independent lighting logic: colors (RGB, HSV), white temperature,
//! gamma correction, transitions, the lamp's state, pixel-by-pixel effects
//! (status signals, ambiances) and the scene that turns them into frames.
//!
//! This crate is `no_std` so it can run on the ESP32-C6, but its tests run
//! on the host machine with a plain `cargo test -p led_core`.

// Use the standard library only when compiling tests on the host.
#![cfg_attr(not(test), no_std)]

pub mod ambiance;
pub mod color;
pub mod effect;
pub mod gamma;
pub mod hsv;
pub mod light;
pub mod scene;
pub mod transition;
pub mod white;

// Re-exports so users can write `led_core::Rgb` instead of `led_core::color::Rgb`.
pub use ambiance::Ambiance;
pub use color::{Rgb, Rgbcct};
pub use effect::{Scanner, StatusSignal};
pub use gamma::gamma8;
pub use hsv::Hsv;
pub use light::{LightColor, LightState};
pub use scene::{FadeTimes, Scene};
pub use transition::{Easing, Transition};
pub use white::{ColorTemperature, WhiteChannels, WhiteMix};
