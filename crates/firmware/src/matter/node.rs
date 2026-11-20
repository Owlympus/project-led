//! What the lamp exposes to Matter: its endpoints and their clusters, and the
//! filters that route each request to the right handler.

use led_core::Ambiance;
use rs_matter_embassy::matter::dm::clusters::app::color_control::ColorControlHooks;
use rs_matter_embassy::matter::dm::clusters::app::level_control::LevelControlHooks;
use rs_matter_embassy::matter::dm::clusters::app::on_off::OnOffHooks;
use rs_matter_embassy::matter::dm::clusters::decl::scenes_management::FULL_CLUSTER as SCENES_CLUSTER;
use rs_matter_embassy::matter::dm::clusters::desc::{self, ClusterHandler as _};
use rs_matter_embassy::matter::dm::clusters::identify;
use rs_matter_embassy::matter::dm::devices::{
    DEV_TYPE_EXTENDED_COLOR_LIGHT, DEV_TYPE_ON_OFF_LIGHT,
};
use rs_matter_embassy::matter::dm::{Cluster, Endpoint, Node};
use rs_matter_embassy::matter::{clusters, devices};
use rs_matter_embassy::stack::MatterStack;
use rs_matter_embassy::wireless::EmbassyWifiNetwork;

use super::ambiance::AmbianceSwitch;
use super::color::ColorLight;
use super::level::DimmableLight;
use super::on_off::OnOffLight;

/// Endpoint 0 is reserved for Matter's own system clusters: the light is on
/// endpoint 1.
pub(super) const LIGHT_ENDPOINT_ID: u16 = 1;

/// The ambiance switches: endpoints 2, 3 and 4, in the order of
/// [`Ambiance::ALL`] (Sunset, Aurora, Campfire). Sized by
/// [`Ambiance::COUNT`]: a new ambiance won't compile until it has its
/// endpoint.
pub(super) const AMBIANCE_ENDPOINT_IDS: [u16; Ambiance::COUNT] = [2, 3, 4];

/// The root endpoint (system clusters), one Extended Color Light and one
/// switch per ambiance.
///
/// The device type also asks for the Groups cluster, left out for now: it
/// needs `rs-matter`'s `groups` feature (multicast), and the Home app should
/// not need it (ADR-0010).
pub(super) const NODE: Node = Node {
    endpoints: &[
        MatterStack::<0, EmbassyWifiNetwork>::wifi_root_endpoint(),
        Endpoint::new(
            LIGHT_ENDPOINT_ID,
            devices!(DEV_TYPE_EXTENDED_COLOR_LIGHT),
            clusters!(
                desc::DescHandler::CLUSTER,
                identify::CLUSTER,
                OnOffLight::CLUSTER,
                DimmableLight::CLUSTER,
                ColorLight::CLUSTER,
                SCENES_CLUSTER,
            ),
        ),
        ambiance_endpoint(AMBIANCE_ENDPOINT_IDS[0]),
        ambiance_endpoint(AMBIANCE_ENDPOINT_IDS[1]),
        ambiance_endpoint(AMBIANCE_ENDPOINT_IDS[2]),
    ],
};

/// An ambiance switch's endpoint: an On/Off Light for the Home app.
const fn ambiance_endpoint(id: u16) -> Endpoint<'static> {
    Endpoint::new(
        id,
        devices!(DEV_TYPE_ON_OFF_LIGHT),
        clusters!(
            desc::DescHandler::CLUSTER,
            identify::CLUSTER,
            AmbianceSwitch::CLUSTER,
        ),
    )
}

/// Whether a request for (`endpoint`, `cluster`) is for `wanted` on the light.
///
/// A plain function, not a closure: `chain` only accepts filters that
/// capture nothing (Rust turns them into simple function pointers).
pub(super) fn on_light(endpoint: u16, cluster: u32, wanted: &Cluster) -> bool {
    endpoint == LIGHT_ENDPOINT_ID && cluster == wanted.id
}

/// Whether a request is for the On/Off cluster of the ambiance switch on
/// endpoint `E`.
///
/// `<const E: u16>` is a *const generic*: the compiler makes one copy of the
/// function per value used (`on_ambiance::<2>`, `::<3>`…). Each copy is a
/// plain function, which `chain` accepts, unlike a closure that would
/// capture the endpoint.
pub(super) fn on_ambiance<const E: u16>(endpoint: u16, cluster: u32) -> bool {
    endpoint == E && cluster == AmbianceSwitch::CLUSTER.id
}
