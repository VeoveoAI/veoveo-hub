pub use crate::contract::HubServerHealthState;

use chrono::{DateTime, Utc};

/// A registered server's health: `checked_at` is when the Hub last saw its
/// registration's login hold a live kernel instance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HubServerHealth {
    pub state: HubServerHealthState,
    pub checked_at: DateTime<Utc>,
}
