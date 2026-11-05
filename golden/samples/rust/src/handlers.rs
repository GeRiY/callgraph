//! HTTP handlers for the order API (axum).

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use tracing::info;

use crate::shapes::Shape;
use crate::state::{AppState, OrderStatus};

/// Request body of `POST /orders`.
#[derive(Debug, Deserialize)]
pub struct CreateOrder {
    pub item: String,
    pub quantity: u32,
}

/// The order routes.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/orders", post(create_order))
        .route("/orders/{id}", get(show_order))
        .with_state(state)
}

/// Creates an order and returns its id.
pub async fn create_order(
    State(state): State<AppState>,
    Json(body): Json<CreateOrder>,
) -> Result<Json<u64>, StatusCode> {
    if body.quantity == 0 {
        return Err(StatusCode::BAD_REQUEST);
    }
    // TODO: validate the item against the catalogue
    let id = state.next_id();
    info!("order {id} created for {}", body.item);
    Ok(Json(id))
}

/// Looks an order up by id.
pub async fn show_order(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Result<String, StatusCode> {
    match state.status(id) {
        Some(OrderStatus::Shipped { carrier }) => Ok(format!("shipped via {carrier}")),
        Some(status) => Ok(status.label().to_owned()),
        None => Err(StatusCode::NOT_FOUND),
    }
}

/// Area of a parcel (the shapes module is reused for packaging).
pub fn parcel_area(shape: &dyn Shape) -> f64 {
    shape.area().max(0.0)
}
