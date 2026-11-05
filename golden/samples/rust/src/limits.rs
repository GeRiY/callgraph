//! Order limits, and a small client over the shared state that uses them.

use crate::state::AppState;

/// Maximum number of items in one order.
pub const MAX_ITEMS: u32 = 50;

/// Banner shown on the status page.
pub static BANNER: &str = "Sample shop is open for business";

/// Raw greeting bytes.
const GREETING: &[u8] = b"Hello from the raw bytes";

/// A client over the shared state.
pub struct Client {
    state: AppState,
}

impl Client {
    /// How many times an id allocation is retried.
    pub const RETRIES: u8 = 3;

    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    /// Allocates an order id (retried a few times).
    pub fn allocate(&self) -> u64 {
        let mut id = 0;
        for _ in 0..Self::RETRIES {
            id = self.state.next_id();
        }
        id
    }

    /// Whether the quantity fits into one order.
    pub fn within_limit(&self, quantity: u32) -> bool {
        quantity <= MAX_ITEMS && !GREETING.is_empty()
    }

    fn banner_len(&self) -> usize {
        BANNER.len()
    }
}

/// One new id per client (the closure parameter's type is not inferred: `?::next_id`).
pub fn allocate_all(clients: &[Client]) -> Vec<u64> {
    clients.iter().map(|c| c.state.next_id()).collect()
}
