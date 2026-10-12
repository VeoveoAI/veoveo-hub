pub use veoveo_gateway_contract::GatewayServerHealthState;

use chrono::{DateTime, Utc};

/// A registered server's lease health: `checked_at` is its last renewal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GatewayServerHealth {
    pub state: GatewayServerHealthState,
    pub checked_at: DateTime<Utc>,
}
