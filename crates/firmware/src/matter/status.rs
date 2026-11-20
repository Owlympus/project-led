//! The connection's status signals on the strip (step 4e, ADR-0013 and
//! ADR-0015): [`watch_status`] picks the one to show, [`WifiWatcher`] tells
//! it whether the Wi-Fi is up.

use core::sync::atomic::{AtomicBool, Ordering};

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Instant, Timer};
use led_core::StatusSignal;
use log::{info, warn};
use rs_matter_embassy::matter::Matter;
use rs_matter_embassy::matter::dm::clusters::gen_diag::NetifDiag;
use rs_matter_embassy::matter::dm::networks::NetChangeNotif;
use rs_matter_embassy::matter::error::Error;
use rs_matter_embassy::stack::UserTask;
use rs_matter_embassy::stack::net::NetStack;

/// The status signal to show on the strip (`None`: just the lamp's color).
/// Sent only when it changes.
pub static STATUS_CHANGED: Signal<CriticalSectionRawMutex, Option<StatusSignal>> = Signal::new();

/// Whether the Wi-Fi is up right now, and whether it has been since power-up
/// (see [`WifiWatcher`]). An `AtomicBool` is a `bool` that several tasks can
/// read and write safely, without a mutex.
static WIFI_UP: AtomicBool = AtomicBool::new(false);
static WIFI_WAS_UP: AtomicBool = AtomicBool::new(false);

/// How often [`watch_status`] looks at the connection.
const STATUS_POLL: Duration = Duration::from_millis(250);

/// Picks the status signal to show, and tells the strip when it changes.
/// Never returns.
///
/// It asks Matter a few times per second rather than waiting for events:
/// simple, and cheap enough (two flags and a list of fabrics to look at).
pub(super) async fn watch_status(matter: &Matter<'_>) -> ! {
    let mut shown = None;
    // Whether the lamp was paired and on the Wi-Fi at the last look, and
    // until when the blue "connected" signal shows.
    let mut was_synced = false;
    let mut connected_until = Instant::now();
    loop {
        let now = Instant::now();
        // A "fabric" is a Matter network the lamp belongs to; ours is the
        // Apple TV's. No fabric: not paired yet.
        let paired = matter.has_fabrics();
        let wifi_up = WIFI_UP.load(Ordering::Relaxed);
        let synced = paired && wifi_up;
        if synced && !was_synced {
            // Just became fully connected (after pairing, at power-up, or
            // when the Wi-Fi comes back): one blue round trip.
            connected_until = now + Duration::from_millis(StatusSignal::CONNECTED_MS.into());
        }
        was_synced = synced;

        let status = if !paired {
            Some(StatusSignal::WaitingForPairing)
        } else if synced && now < connected_until {
            Some(StatusSignal::Connected)
        } else if !wifi_up && WIFI_WAS_UP.load(Ordering::Relaxed) {
            Some(StatusSignal::WifiLost)
        } else {
            // All good, or paired and still joining the Wi-Fi after power-up:
            // just the lamp's color.
            None
        };

        if status != shown {
            info!("status signal: {status:?}");
            STATUS_CHANGED.signal(status);
            shown = status;
        }
        Timer::after(STATUS_POLL).await;
    }
}

/// Keeps [`WIFI_UP`] up to date, as a Matter "user task".
///
/// `rs-matter` hands this task the network interface (`netif`). It runs from
/// startup, Wi-Fi or not (first tests on the board, ADR-0015): so it reads
/// the interface's real state, *operational* or not, and waits for it to
/// change.
pub(super) struct WifiWatcher;

impl UserTask for WifiWatcher {
    async fn run<S, N>(&mut self, _net_stack: S, netif: N) -> Result<(), Error>
    where
        S: NetStack,
        N: NetifDiag + NetChangeNotif,
    {
        // If `rs-matter` ever stops this task, assume the Wi-Fi is gone.
        let _down_on_stop = WifiDownOnDrop;
        loop {
            // `netifs` calls the closure once per interface (just the Wi-Fi
            // here). `|=`: up if any interface is operational.
            let mut up = false;
            netif.netifs(&mut |info| {
                up |= info.operational;
                Ok(())
            })?;
            set_wifi_up(up);
            // Sleep until the interface changes (link, addresses…).
            netif.wait_changed().await;
        }
    }
}

/// Updates [`WIFI_UP`] (and [`WIFI_WAS_UP`]), logging only the changes.
fn set_wifi_up(up: bool) {
    // `swap` writes the new value and returns the old one, in one step.
    let was = WIFI_UP.swap(up, Ordering::Relaxed);
    if up {
        WIFI_WAS_UP.store(true, Ordering::Relaxed);
    }
    match (was, up) {
        (false, true) => info!("Wi-Fi: up"),
        (true, false) => warn!("Wi-Fi: down"),
        _ => {}
    }
}

/// Lowers [`WIFI_UP`] when dropped, that is when [`WifiWatcher`]'s task is
/// stopped.
///
/// `Drop` is Rust's destructor, like PHP's `__destruct`, but it runs at a
/// known moment (when the value goes away), never "later".
struct WifiDownOnDrop;

impl Drop for WifiDownOnDrop {
    fn drop(&mut self) {
        set_wifi_up(false);
    }
}
