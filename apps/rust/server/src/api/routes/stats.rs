use std::sync::Arc;

use axum::{extract::State, response::Response};
use backend_persistence::StatsOverview;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    AppState,
    api::{error::AppError, response_cache},
    app::stats_projection,
};

pub fn router() -> OpenApiRouter<Arc<AppState>> {
    OpenApiRouter::new().routes(routes!(get_stats_overview))
}

#[utoipa::path(
    get,
    path = "/v1/stats",
    tag = "stats",
    responses(
        (status = OK, body = StatsOverview),
        (status = INTERNAL_SERVER_ERROR, body = crate::api::dto::ErrorEnvelopeResponse)
    )
)]
async fn get_stats_overview(State(state): State<Arc<AppState>>) -> Result<Response, AppError> {
    response_cache::cached_json(&state.cache, "stats:overview", || async {
        stats_projection::overview(&state).await
    })
    .await
}
