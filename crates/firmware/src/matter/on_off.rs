//! Our side of the main light's On/Off cluster.

use log::info;
use rs_matter_embassy::matter::dm::Cluster;
use rs_matter_embassy::matter::dm::clusters::app::on_off::{
    EffectVariantEnum, OnOffHooks, OutOfBandMessage,
};
use rs_matter_embassy::matter::dm::clusters::decl::on_off as on_off_cluster;
use rs_matter_embassy::matter::utils::sync::MaybeSend;
use rs_matter_embassy::matter::with;

use super::level::RESTORE_LEVEL;
use super::light::{LIGHT_REQUEST, LightRequest, update_light};

/// It holds no state: the cluster keeps the on/off value, we only pass it on.
pub(super) struct OnOffLight;

/// `OnOffHooks` is a trait (like a PHP interface): `rs-matter` calls these
/// methods, we decide what they do.
impl OnOffHooks for OnOffLight {
    /// The cluster description. A light must have the `LIGHTING` feature,
    /// which brings a few extra attributes and commands; `rs-matter`
    /// implements them all, we just list them.
    const CLUSTER: Cluster<'static> = on_off_cluster::FULL_CLUSTER
        .with_revision(6)
        .with_features(on_off_cluster::Feature::LIGHTING.bits())
        .with_attrs(with!(
            required;
            on_off_cluster::AttributeId::OnOff
                | on_off_cluster::AttributeId::GlobalSceneControl
                | on_off_cluster::AttributeId::OnTime
                | on_off_cluster::AttributeId::OffWaitTime
                | on_off_cluster::AttributeId::StartUpOnOff
        ))
        .with_cmds(with!(
            on_off_cluster::CommandId::Off
                | on_off_cluster::CommandId::On
                | on_off_cluster::CommandId::Toggle
                | on_off_cluster::CommandId::OffWithEffect
                | on_off_cluster::CommandId::OnWithRecallGlobalScene
                | on_off_cluster::CommandId::OnWithTimedOff
        ));

    fn set_on_off(&self, on: bool) {
        info!("Matter: light {}", if on { "on" } else { "off" });
        // Switching the lamp off also leaves its ambiance.
        update_light(|light| {
            light.on = on;
            if !on {
                light.ambiance = None;
            }
        });
        if !on {
            // Let the brightness side check where it was left (see
            // `DimmableLight::run`).
            RESTORE_LEVEL.signal(());
        }
    }

    async fn handle_off_with_effect(&self, _effect: EffectVariantEnum) {
        // Special "off" animations (fade, dying light…): not for now.
    }

    /// Runs alongside the cluster, for requests that don't come from the
    /// network (touch button, ambiance switches). `notify` hands them to the
    /// cluster, which then applies them like a Home app command (fade, saved
    /// state, Home app updated). Must never return.
    async fn run<F: Fn(OutOfBandMessage) + MaybeSend>(&self, notify: F) {
        loop {
            notify(match LIGHT_REQUEST.wait().await {
                LightRequest::Toggle => OutOfBandMessage::Toggle,
                LightRequest::On => OutOfBandMessage::On,
                LightRequest::Off => OutOfBandMessage::Off,
            });
        }
    }
}
