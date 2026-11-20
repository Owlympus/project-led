//! Our side of an ambiance switch's On/Off cluster (step 4f, ADR-0014).

use led_core::Ambiance;
use log::info;
use rs_matter_embassy::matter::dm::Cluster;
use rs_matter_embassy::matter::dm::clusters::app::on_off::{
    EffectVariantEnum, OnOffHooks, OutOfBandMessage,
};
use rs_matter_embassy::matter::dm::clusters::decl::on_off as on_off_cluster;
use rs_matter_embassy::matter::utils::sync::MaybeSend;

use super::light::{AMBIANCE_SWITCH_SHOWS, LIGHT_REQUEST, LightRequest, light, update_light};
use super::on_off::OnOffLight;

pub(super) struct AmbianceSwitch(pub(super) Ambiance);

impl OnOffHooks for AmbianceSwitch {
    /// Same description as the main light's On/Off cluster.
    const CLUSTER: Cluster<'static> = OnOffLight::CLUSTER;
    /// Ambiances always start off after a power cut: the lamp comes back in
    /// its plain color (ADR-0014).
    const START_UP_ON_OFF: Option<on_off_cluster::StartUpOnOffEnum> =
        Some(on_off_cluster::StartUpOnOffEnum::Off);

    fn set_on_off(&self, on: bool) {
        // `self.0`: the first (and only) field of the tuple struct.
        let ambiance = self.0;
        if on {
            info!("ambiance on: {}", ambiance.name());
            // Only one ambiance at a time: `update_light` switches the
            // previous one's switch off.
            let before = update_light(|light| light.ambiance = Some(ambiance));
            // An ambiance on a lamp that is off: switch the lamp on too
            // (through Matter, so the Home app follows and the fade applies).
            if !before.on {
                LIGHT_REQUEST.signal(LightRequest::On);
            }
        } else if light().ambiance == Some(ambiance) {
            // Switching off the ambiance shown switches the lamp off
            // (ADR-0016), through Matter so the Home app follows and the
            // fade applies. The ambiance stays on the strip during the fade
            // (no flash of the plain color); switching the lamp off then
            // leaves it.
            info!("ambiance off: {} (its switch), lamp off", ambiance.name());
            LIGHT_REQUEST.signal(LightRequest::Off);
        }
    }

    async fn handle_off_with_effect(&self, _effect: EffectVariantEnum) {}

    /// Shows the switch on or off in the Home app when another part of the
    /// lamp changed the ambiance. `Update` only changes what Matter reports:
    /// it doesn't call `set_on_off` again (no loop).
    async fn run<F: Fn(OutOfBandMessage) + MaybeSend>(&self, notify: F) {
        loop {
            let on = AMBIANCE_SWITCH_SHOWS[self.0.index()].wait().await;
            notify(OutOfBandMessage::Update(on));
        }
    }
}
