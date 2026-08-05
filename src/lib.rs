//! Sigma Catalog: SKU directory with simple and composite items.

#![forbid(unsafe_code)]

mod api;
pub mod config;
mod model;
mod session_status;
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
    config::validate_with(&[sigma_config::DATABASE_URL_VAR])?;
    let store = store::CatalogStore::connect().await?;
    let addr = sigma_theme::warp::listen_addr_from_env();
    sigma_theme::warp::serve("Sigma Catalog", addr, routes(store)).await?;
    Ok(())
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
        web::routes(sigma_theme::warp::with_state(store.clone()))
            .or(api::routes(sigma_theme::warp::with_state(store))),
        sigma_pg::health::warp::health_routes("catalog", Some(health_pool)),
    )
    .with(warp::reply::with::headers(security_header_map()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use warp::http::StatusCode;

    async fn test_store() -> store::CatalogStore {
        sigma_pg::test_helpers::ready_store(store::CatalogStore::connect_empty()).await
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
    async fn index_without_session_redirects_to_sign_in() {
        let res = warp::test::request()
            .method("GET")
            .path("/")
            .reply(&routes(test_store().await))
            .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        let location = res
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        assert!(location.contains("/auth/login"));
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
    async fn api_create_composite_and_list_includes_components() {
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

        let kit_res = warp::test::request()
            .method("POST")
            .path("/skus")
            .header("content-type", "application/json")
            .header("x-sigma-internal-token", sigma_pg::clients::internal::TEST_INTERNAL_TOKEN)
            .body(format!(
                r#"{{"sku_code":"KIT-02","name":"Form kit","description":null,"category":null,"kind":"composite","active":true,"components":[{{"sku_id":"{}","quantity":3}}]}}"#,
                part.id
            ))
            .reply(&app)
            .await;
        assert_eq!(kit_res.status(), StatusCode::CREATED);
        let kit: Sku = serde_json::from_slice(kit_res.body()).unwrap();
        assert_eq!(kit.sku_code, "KIT-02");
        assert_eq!(kit.components.len(), 1);
        assert_eq!(kit.components[0].quantity, 3);

        let list_res = warp::test::request()
            .method("GET")
            .path("/skus")
            .header("accept", "application/json")
            .header("x-sigma-internal-token", sigma_pg::clients::internal::TEST_INTERNAL_TOKEN)
            .reply(&app)
            .await;
        assert_eq!(list_res.status(), StatusCode::OK);
        let skus: Vec<Sku> = serde_json::from_slice(list_res.body()).unwrap();
        let listed_kit = skus.iter().find(|s| s.sku_code == "KIT-02").unwrap();
        assert_eq!(listed_kit.components.len(), 1);
        assert_eq!(listed_kit.components[0].sku_id, part.id);
    }

    #[tokio::test]
    async fn web_form_post_without_session_redirects_to_sign_in() {
        let app = routes(test_store().await);

        let res = warp::test::request()
            .method("POST")
            .path("/skus")
            .header("content-type", "application/x-www-form-urlencoded")
            .body("sku_code=KIT-03&name=Form+kit&description=&category=&kind=simple&active=on")
            .reply(&app)
            .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        let location = res
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        assert!(location.contains("/auth/login"));
    }

    #[tokio::test]
    async fn new_sku_page_without_session_redirects_to_sign_in() {
        let app = routes(test_store().await);

        let res = warp::test::request()
            .method("GET")
            .path("/skus/new")
            .reply(&app)
            .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        let location = res
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        assert!(location.contains("/auth/login"));
    }
}
