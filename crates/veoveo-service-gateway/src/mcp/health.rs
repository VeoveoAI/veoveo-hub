pub use crate::contract::GatewayServerHealthState;

use chrono::{DateTime, Utc};

/// A registered server's health: `checked_at` is when the Gateway last saw its
/// registration's login hold a live kernel instance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GatewayServerHealth {
    pub state: GatewayServerHealthState,
    pub checked_at: DateTime<Utc>,
}
