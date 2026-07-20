//! Sigma Catalog: SKU directory with simple and composite items.

#![forbid(unsafe_code)]

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
use warp::http::header::{HeaderName, HeaderValue};

pub use model::{CreateSku, Sku, SkuComponent, SkuKind, UpdateSku};

/// Shared catalog store handle (`PgPool` is internally concurrent).
pub type SharedStore = Arc<store::CatalogStore>;

/// Connect to PostgreSQL and serve the site until a shutdown signal arrives.
///
/// # Errors
///
/// Returns an error when the database connection or binding the listen
/// address fails.
pub async fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let store = store::CatalogStore::connect().await?;
    let addr = sigma_theme::warp::listen_addr_from_env();
    sigma_theme::warp::serve("Sigma Catalog", addr, routes(store)).await?;
    Ok(())
}

fn with_store(
    store: SharedStore,
) -> impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone {
    warp::any().map(move || store.clone())
}

/// Local CSP: the shared `sigma_theme::warp::security_headers` helper hard-codes
/// `style-src 'self'`, and the SKU form relies on inline `style` attributes.
fn content_security_policy() -> String {
    let identity_origin = config::identity_public_origin();
    format!(
        "default-src 'self'; base-uri 'self'; object-src 'none'; frame-ancestors 'none'; \
         img-src 'self' data:; style-src 'self' 'unsafe-inline'; script-src 'self'; \
         font-src 'self'; connect-src 'self' {identity_origin}; form-action 'self'"
    )
}

/// Local CSP plus the shared security header set (see
/// [`sigma_theme::SECURITY_HEADERS`]).
fn security_header_map() -> warp::http::HeaderMap {
    let mut map = warp::http::HeaderMap::new();
    map.insert(
        warp::http::header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_str(&content_security_policy()).expect("valid CSP header value"),
    );
    for (name, value) in sigma_theme::SECURITY_HEADERS {
        map.insert(
            HeaderName::from_static(name),
            HeaderValue::from_static(value),
        );
    }
    map
}

/// Site routes: web UI, JSON API, `/up`, theme static assets, error recovery.
pub fn routes(
    store: store::CatalogStore,
) -> impl Filter<Extract = (impl Reply,), Error = Infallible> + Clone + Send + 'static {
    let health_pool = Arc::new(store.pool().clone());
    let store = Arc::new(store);

    sigma_theme::warp::site_routes(
        web::routes(with_store(store.clone())).or(api::routes(with_store(store))),
        sigma_pg::health::warp::health_routes("catalog", Some(health_pool)),
    )
    .with(warp::reply::with::headers(security_header_map()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use warp::http::StatusCode;

    async fn test_store() -> store::CatalogStore {
        sigma_pg::clients::internal::ensure_test_internal_token();
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

    #[test]
    fn csp_allows_identity_status_fetch() {
        let csp = content_security_policy();
        assert!(
            csp.contains("connect-src 'self' http://127.0.0.1:3000"),
            "csp should allow identity origin, got: {csp}"
        );
    }

    #[tokio::test]
    async fn responses_carry_shared_security_headers() {
        let res = warp::test::request()
            .method("GET")
            .path("/up")
            .reply(&routes(test_store().await))
            .await;
        for (name, value) in sigma_theme::SECURITY_HEADERS {
            assert_eq!(res.headers().get(*name).unwrap(), value, "header {name}");
        }
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
            .header(
                "x-sigma-internal-token",
                sigma_pg::clients::internal::TEST_INTERNAL_TOKEN,
            )
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
            .header("x-sigma-internal-token", sigma_pg::clients::internal::TEST_INTERNAL_TOKEN)
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
            .header("x-sigma-internal-token", sigma_pg::clients::internal::TEST_INTERNAL_TOKEN)
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
            .header("x-sigma-internal-token", sigma_pg::clients::internal::TEST_INTERNAL_TOKEN)
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

    #[tokio::test]
    async fn web_form_creates_composite_and_index_lists_components() {
        let store = test_store().await;
        let app = routes(store);

        let part_res = warp::test::request()
            .method("POST")
            .path("/skus")
            .header("content-type", "application/json")
            .header("x-sigma-internal-token", sigma_pg::clients::internal::TEST_INTERNAL_TOKEN)
            .body(
                r#"{"sku_code":"PART-B","name":"Part B","description":null,"category":null,"kind":"simple","active":true,"components":[]}"#,
            )
            .reply(&app)
            .await;
        let part: Sku = serde_json::from_slice(part_res.body()).unwrap();

        let res = warp::test::request()
            .method("POST")
            .path("/skus")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(format!(
                "sku_code=KIT-02&name=Form+kit&description=&category=&kind=composite&active=on&component={id}&qty_{id}=3",
                id = part.id
            ))
            .reply(&app)
            .await;
        assert!(
            res.status().is_redirection(),
            "expected redirect, got {} — {}",
            res.status(),
            std::str::from_utf8(res.body()).unwrap_or("")
        );

        let index = warp::test::request()
            .method("GET")
            .path("/")
            .reply(&app)
            .await;
        let body = std::str::from_utf8(index.body()).unwrap();
        assert!(body.contains("KIT-02"));
        assert!(body.contains("PART-B"));
        assert!(body.contains("Part B"));
        assert!(body.contains("× 3"));
    }

    #[tokio::test]
    async fn form_page_offers_component_checkboxes() {
        let store = test_store().await;
        let app = routes(store);

        warp::test::request()
            .method("POST")
            .path("/skus")
            .header("content-type", "application/json")
            .header("x-sigma-internal-token", sigma_pg::clients::internal::TEST_INTERNAL_TOKEN)
            .body(
                r#"{"sku_code":"PART-C","name":"Part C","description":null,"category":null,"kind":"simple","active":true,"components":[]}"#,
            )
            .reply(&app)
            .await;

        let res = warp::test::request()
            .method("GET")
            .path("/skus/new")
            .reply(&app)
            .await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = std::str::from_utf8(res.body()).unwrap();
        assert!(body.contains(r#"name="component""#));
        assert!(body.contains("PART-C"));
        assert!(body.contains("Part C"));
    }
}
