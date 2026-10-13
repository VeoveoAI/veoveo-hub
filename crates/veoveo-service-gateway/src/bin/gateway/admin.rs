#[path = "admin/console/mod.rs"]
mod console;
pub(crate) use console::{ConsoleStreamLimits, audit as console_audit};
#[path = "admin/control_plane.rs"]
mod control_plane;
#[path = "admin/jwt_revocations.rs"]
mod jwt_revocations;
#[path = "admin/server_health.rs"]
mod server_health;
#[path = "admin/tasks.rs"]
mod tasks;

use veoveo_mcp_contract::GatewayProfileId;

pub(super) use control_plane::{read_control_plane, update_control_plane};
pub(super) use jwt_revocations::{prune_jwt_revocations, revoke_jwt};
pub(super) use server_health::read_server_health;
pub(super) use tasks::cancel_task;

fn admin_profile_id(profile: String) -> Option<GatewayProfileId> {
    GatewayProfileId::parse(profile).ok()
}
