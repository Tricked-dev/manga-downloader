mod clients;
mod downloads;
mod info;
mod library;
mod maintenance;
mod proxy;
mod settings;
mod sources;
mod stats;

use std::sync::Arc;

use utoipa_axum::router::OpenApiRouter;

use crate::{AppState, api::openapi};

pub fn router() -> OpenApiRouter<Arc<AppState>> {
    openapi::openapi_router()
        .merge(info::router())
        .merge(sources::router())
        .merge(library::router())
        .merge(downloads::router())
        .merge(clients::router())
        .merge(settings::router())
        .merge(maintenance::router())
        .merge(stats::router())
        .merge(proxy::router())
}
