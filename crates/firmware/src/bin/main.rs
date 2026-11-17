//! Firmware entry point for the ESP32-C6.
//!
//! For now it only proves that the toolchain works end to end:
//! - Embassy runs two tasks side by side (a heartbeat and a color demo);
//! - `led_core` (our hardware-independent crate) runs on the chip;
//! - logs go to the computer over USB (`cargo run` flashes and shows them).
//!
//! Nothing is wired yet: driving the on-board RGB LED and then the WS2805
//! strip are the next steps.

// No standard library (no OS on the chip), and no regular `fn main()`:
// `#[esp_rtos::main]` below provides the real entry point.
#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_backtrace as _; // prints a backtrace over USB if the program panics
use esp_hal::clock::CpuClock;
use esp_hal::timer::timg::TimerGroup;
use led_core::{Hsv, Rgb};
use log::info;

// Metadata required by the bootloader (version, build date…).
esp_bootloader_esp_idf::esp_app_desc!();

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    esp_println::logger::init_logger_from_env();

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // Embassy needs a hardware timer to know when to wake tasks up.
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);
    info!("Embassy started");

    // Runs `heartbeat` concurrently with the rest of `main`.
    spawner.spawn(heartbeat().expect("heartbeat task already running"));

    // Color demo: walk around the color wheel, computed by `led_core`.
    let mut degrees: u16 = 0;
    loop {
        let rgb: Rgb = Hsv::from_degrees(degrees, 255, 255).into();
        info!(
            "hue {degrees:>3}° -> {:?} -> strip {:?}",
            rgb,
            rgb.gamma_corrected()
        );
        degrees = (degrees + 30) % 360;
        Timer::after(Duration::from_millis(500)).await;
    }
}

/// Logs a message every 2 seconds, to show that tasks run side by side.
#[embassy_executor::task]
async fn heartbeat() {
    let mut beats: u32 = 0;
    loop {
        beats += 1;
        info!("heartbeat #{beats}");
        Timer::after(Duration::from_secs(2)).await;
    }
}
