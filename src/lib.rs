//! Sigma Catalog: SKU directory with simple and composite items.

mod api;
pub mod config;
mod model;
pub mod store;
mod templates;
mod web;

use std::convert::Infallible;
use std::sync::Arc;

use warp::Filter;
use warp::Reply;

pub use model::{CreateSku, Sku, SkuComponent, SkuKind, UpdateSku};

/// Shared catalog store handle (`PgPool` is internally concurrent).
pub type SharedStore = Arc<store::CatalogStore>;

/// Resolve listen address from **`PORT`** (default **8080**).
#[must_use]
pub fn listen_socket_addr_from_env() -> std::net::SocketAddr {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8080);
    SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port)
}

fn with_store(
    store: SharedStore,
) -> impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone {
    warp::any().map(move || store.clone())
}

fn content_security_policy() -> String {
    let identity_origin = config::identity_public_origin();
    format!(
        "default-src 'self'; base-uri 'self'; object-src 'none'; frame-ancestors 'none'; \
         img-src 'self' data:; style-src 'self' 'unsafe-inline'; script-src 'self'; \
         font-src 'self'; connect-src 'self' {identity_origin}; form-action 'self'"
    )
}

/// Site routes: web UI, JSON API, `/up`, theme static assets, error recovery.
pub fn routes(
    store: store::CatalogStore,
) -> impl Filter<Extract = (impl Reply,), Error = Infallible> + Clone + Send + 'static {
    use warp::reply::with::header;

    let health_pool = Arc::new(store.pool().clone());
    let store = Arc::new(store);

    warp::path("up")
        .and(warp::get())
        .map(|| warp::reply::with_status("up", warp::http::StatusCode::OK))
        .or(sigma_pg::health::warp::health_routes(
            "catalog",
            Some(health_pool),
        ))
        .or(web::routes(with_store(store.clone())))
        .or(api::routes(with_store(store)))
        .or(sigma_theme::warp::static_files())
        .or(sigma_theme::warp::favicon())
        .recover(sigma_theme::warp::handle_rejection)
        .with(header("content-security-policy", content_security_policy()))
        .with(header("x-content-type-options", "nosniff"))
        .with(header("x-frame-options", "DENY"))
        .with(header("referrer-policy", "strict-origin-when-cross-origin"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use warp::http::StatusCode;

    async fn test_store() -> store::CatalogStore {
        store::CatalogStore::connect_empty()
            .await
            .expect("PostgreSQL required for tests")
    }

    #[tokio::test]
    async fn up_returns_ok() {
        let res = warp::test::request()
            .method("GET")
            .path("/up")
            .reply(&routes(test_store().await))
            .await;
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn index_lists_skus() {
        let res = warp::test::request()
            .method("GET")
            .path("/")
            .reply(&routes(test_store().await))
            .await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = std::str::from_utf8(res.body()).unwrap();
        assert!(body.contains("Catalog"));
        assert!(body.contains("id=\"store-nav-auth\""));
    }

    #[tokio::test]
    async fn api_lists_empty_skus() {
        let res = warp::test::request()
            .method("GET")
            .path("/skus")
            .header("accept", "application/json")
            .reply(&routes(test_store().await))
            .await;
        assert_eq!(res.status(), StatusCode::OK);
        let body: Vec<Sku> = serde_json::from_slice(res.body()).unwrap();
        assert!(body.is_empty());
    }

    #[tokio::test]
    async fn api_create_simple_sku() {
        let res = warp::test::request()
            .method("POST")
            .path("/skus")
            .header("content-type", "application/json")
            .body(
                r#"{"sku_code":"WIDGET-01","name":"Widget","description":null,"category":"parts","kind":"simple","active":true,"components":[]}"#,
            )
            .reply(&routes(test_store().await))
            .await;
        assert_eq!(res.status(), StatusCode::CREATED);
        let sku: Sku = serde_json::from_slice(res.body()).unwrap();
        assert_eq!(sku.sku_code, "WIDGET-01");
        assert_eq!(sku.kind, SkuKind::Simple);
    }

    #[tokio::test]
    async fn api_create_composite_sku() {
        let store = test_store().await;
        let app = routes(store);

        let part_res = warp::test::request()
            .method("POST")
            .path("/skus")
            .header("content-type", "application/json")
            .body(
                r#"{"sku_code":"PART-A","name":"Part A","description":null,"category":null,"kind":"simple","active":true,"components":[]}"#,
            )
            .reply(&app)
            .await;
        let part: Sku = serde_json::from_slice(part_res.body()).unwrap();

        let res = warp::test::request()
            .method("POST")
            .path("/skus")
            .header("content-type", "application/json")
            .body(format!(
                r#"{{"sku_code":"KIT-01","name":"Starter kit","description":null,"category":null,"kind":"composite","active":true,"components":[{{"sku_id":"{}","quantity":2}}]}}"#,
                part.id
            ))
            .reply(&app)
            .await;
        assert_eq!(res.status(), StatusCode::CREATED);
        let kit: Sku = serde_json::from_slice(res.body()).unwrap();
        assert_eq!(kit.kind, SkuKind::Composite);
        assert_eq!(kit.components.len(), 1);
    }
}
