use std::sync::Arc;

use axum::{Json, Router, routing::get};
use utoipa::{
    Modify, OpenApi,
    openapi::{
        OpenApi as OpenApiSpec,
        security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
    },
};
use utoipa_axum::router::OpenApiRouter;

#[derive(OpenApi)]
#[openapi(
    modifiers(&SecurityAddon),
    security(("bearer_auth" = [])),
    tags(
        (name = "info", description = "Server information and health"),
        (name = "sources", description = "Manga source plugins and catalog access"),
        (name = "library", description = "Local manga library"),
        (name = "downloads", description = "Download queue and archive management"),
        (name = "clients", description = "External client packages"),
        (name = "settings", description = "Server settings"),
        (name = "stats", description = "User-facing reading and download statistics"),
        (name = "media", description = "Media proxy endpoints")
    )
)]
pub struct ApiDoc;

struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut OpenApiSpec) {
        let Some(components) = openapi.components.as_mut() else {
            return;
        };

        components.add_security_scheme(
            "bearer_auth",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("api-key")
                    .build(),
            ),
        );
    }
}

pub fn openapi_router<S>() -> OpenApiRouter<S>
where
    S: Clone + Send + Sync + 'static,
{
    OpenApiRouter::with_openapi(ApiDoc::openapi())
}

pub fn docs_router(openapi: OpenApiSpec) -> Router {
    let openapi = Arc::new(openapi);
    Router::new().route(
        "/openapi.json",
        get({
            let openapi = Arc::clone(&openapi);
            move || {
                let openapi = Arc::clone(&openapi);
                async move { Json((*openapi).clone()) }
            }
        }),
    )
}
