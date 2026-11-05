//! Shared application state and the order status machine.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// The status of an order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrderStatus {
    Pending,
    Paid { amount: u64 },
    Shipped { carrier: String },
    Cancelled,
}

impl OrderStatus {
    /// A label for the UI.
    pub fn label(&self) -> &'static str {
        match self {
            OrderStatus::Pending => "pending",
            OrderStatus::Paid { .. } => "paid",
            OrderStatus::Shipped { .. } => "shipped",
            OrderStatus::Cancelled => "cancelled",
        }
    }

    /// The next status after an event, if the transition is allowed.
    pub fn next(self, event: &str) -> Option<OrderStatus> {
        match (self, event) {
            (OrderStatus::Pending, "pay") => Some(OrderStatus::Paid { amount: 0 }),
            (OrderStatus::Paid { .. }, "ship") => Some(OrderStatus::Shipped {
                carrier: "post".to_owned(),
            }),
            (status, "cancel") if status != OrderStatus::Cancelled => Some(OrderStatus::Cancelled),
            _ => None,
        }
    }
}

/// Shared state of the HTTP server.
#[derive(Clone, Default)]
pub struct AppState {
    counter: Arc<AtomicU64>,
    orders: Arc<Mutex<HashMap<u64, OrderStatus>>>,
}

impl AppState {
    /// A fresh order id.
    pub fn next_id(&self) -> u64 {
        let id = self.counter.fetch_add(1, Ordering::SeqCst) + 1;
        self.orders.lock().unwrap().insert(id, OrderStatus::Pending);
        id
    }

    pub fn status(&self, id: u64) -> Option<OrderStatus> {
        self.orders.lock().ok()?.get(&id).cloned()
    }
}
