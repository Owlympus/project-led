//! Our side of the Color Control cluster (colors and white temperature).

use led_core::light::matter_scale;
use led_core::{ColorTemperature, LightColor};
use log::debug;
use rs_matter_embassy::matter::dm::Cluster;
use rs_matter_embassy::matter::dm::clusters::app::color_control::{
    ColorCapabilitiesBitmap, ColorControlHooks, SetDeviceColor,
};
use rs_matter_embassy::matter::dm::clusters::decl::color_control as color_cluster;
use rs_matter_embassy::matter::with;

use super::light::update_light;

pub(super) struct ColorLight;

impl ColorControlHooks for ColorLight {
    /// All three color modes: hue/saturation, CIE xy (required by the
    /// Extended Color Light device type) and white temperature.
    const CLUSTER: Cluster<'static> = color_cluster::FULL_CLUSTER
        .with_features(
            color_cluster::Feature::HUE_AND_SATURATION.bits()
                | color_cluster::Feature::XY.bits()
                | color_cluster::Feature::COLOR_TEMPERATURE.bits(),
        )
        .with_attrs(with!(
            required;
            color_cluster::AttributeId::CurrentHue
                | color_cluster::AttributeId::CurrentSaturation
                | color_cluster::AttributeId::CurrentX
                | color_cluster::AttributeId::CurrentY
                | color_cluster::AttributeId::RemainingTime
                | color_cluster::AttributeId::ColorTemperatureMireds
                | color_cluster::AttributeId::ColorTempPhysicalMinMireds
                | color_cluster::AttributeId::ColorTempPhysicalMaxMireds
                | color_cluster::AttributeId::StartUpColorTemperatureMireds
        ))
        .with_cmds(with!(
            color_cluster::CommandId::MoveToHue
                | color_cluster::CommandId::MoveHue
                | color_cluster::CommandId::StepHue
                | color_cluster::CommandId::MoveToSaturation
                | color_cluster::CommandId::MoveSaturation
                | color_cluster::CommandId::StepSaturation
                | color_cluster::CommandId::MoveToHueAndSaturation
                | color_cluster::CommandId::MoveToColor
                | color_cluster::CommandId::MoveColor
                | color_cluster::CommandId::StepColor
                | color_cluster::CommandId::MoveToColorTemperature
                | color_cluster::CommandId::MoveColorTemperature
                | color_cluster::CommandId::StepColorTemperature
                | color_cluster::CommandId::StopMoveStep
        ));

    /// Must match the features above.
    const COLOR_CAPABILITIES: ColorCapabilitiesBitmap = ColorCapabilitiesBitmap::from_bits_truncate(
        ColorCapabilitiesBitmap::HUE_SATURATION.bits()
            | ColorCapabilitiesBitmap::XY.bits()
            | ColorCapabilitiesBitmap::COLOR_TEMPERATURE.bits(),
    );

    /// The whites our strip can make: from its cold LED (fewest mireds) to
    /// its warm LED (most mireds). The Home app's slider uses this range.
    const COLOR_TEMP_PHYSICAL_MIN_MIREDS: u16 = crate::STRIP_WHITE_CHANNELS.cold.mireds();
    const COLOR_TEMP_PHYSICAL_MAX_MIREDS: u16 = crate::STRIP_WHITE_CHANNELS.warm.mireds();

    /// Start in the same white as `LightState::DEFAULT`.
    const COLOR: Option<SetDeviceColor> = Some(SetDeviceColor::ColorTemperature {
        mireds: ColorTemperature::from_kelvin(3000).mireds(),
    });

    /// Called at every step of a transition (every 100 ms), hence a
    /// `debug!` log only: hidden by default.
    fn set_device_color(&self, target: SetDeviceColor) -> Result<(), ()> {
        debug!("Matter: color {target:?}");
        let color = match target {
            SetDeviceColor::ColorTemperature { mireds } => {
                LightColor::Temperature(ColorTemperature::from_mireds(mireds))
            }
            SetDeviceColor::HueSaturation {
                enhanced_hue,
                saturation,
            } => LightColor::HueSaturation {
                // The "enhanced" hue already uses the full u16, like `Hsv`.
                hue: enhanced_hue,
                saturation: matter_scale::saturation(saturation),
            },
            // We have no xy → RGB conversion of our own: `rs-matter`'s
            // turns the point into a hue and a saturation.
            xy @ SetDeviceColor::Xy { .. } => {
                let (hue, saturation) = xy.to_hue_saturation();
                LightColor::HueSaturation {
                    hue,
                    saturation: matter_scale::saturation(saturation),
                }
            }
        };
        // A new color from the Home app leaves the ambiance. (Same color:
        // nothing changes, e.g. when the cluster restores it at startup.)
        update_light(|light| {
            if light.color != color {
                light.color = color;
                light.ambiance = None;
            }
        });
        Ok(())
    }
}
