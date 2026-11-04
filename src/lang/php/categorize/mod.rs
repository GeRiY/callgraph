//! Hívás-kategorizálás (Chain of Responsibility): a szabályok sorban futnak, az első illeszkedő dönt.

mod chain;
mod data;
mod rule;
pub mod rules;

pub use chain::CategoryChain;
pub use rule::{CallInfo, CategoryRule};
