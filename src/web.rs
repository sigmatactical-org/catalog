use std::convert::Infallible;

use sigma_theme::warp::internal_rejection;
use warp::http::StatusCode;
use warp::reply::Response;
use warp::{Filter, Rejection, Reply};

use crate::SharedStore;
use crate::config;
use crate::model::{Sku, SkuForm};
use crate::session_status;
use crate::store::StoreError;
use crate::templates::{self, FormValues};

/// Outcome of the admin session gate for HTML admin routes.
enum AdminGate {
    Allow,
    SignIn(Response),
    /// Signed in but not an admin — hide the admin surface.
    Deny,
}

/// Require an admin identity session. Anonymous users are sent to sign-in;
/// signed-in non-admins get a 404 so the catalog UI stays private.
async fn require_admin(cookie: Option<&str>, return_path: &str) -> AdminGate {
    match session_status::fetch_identity_status(cookie).await {
        Some(status) if status.is_admin => AdminGate::Allow,
        Some(_) => AdminGate::Deny,
        None => AdminGate::SignIn(sign_in_redirect(return_path)),
    }
}

fn sign_in_redirect(return_path: &str) -> Response {
    let links = sigma_identity_nav::auth_links(
        &config::identity_public_base_url(),
        &config::public_base_url(),
        return_path,
    );
    match links.sign_in_url.parse::<warp::http::Uri>() {
        Ok(uri) => warp::redirect::see_other(uri).into_response(),
        Err(_) => warp::reply::with_status(
            warp::reply::html(sigma_theme::errors::internal_server_error_html()),
            StatusCode::INTERNAL_SERVER_ERROR,
        )
        .into_response(),
    }
}

fn cookie_filter() -> impl Filter<Extract = (Option<String>,), Error = Rejection> + Clone {
    warp::header::optional::<String>("cookie")
}

/// Build this module's routes.
pub fn routes(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    index_page(store.clone())
        .or(new_sku_page(store.clone()))
        .or(create_sku_form(store.clone()))
        .or(edit_sku_page(store.clone()))
        .or(update_sku_form(store.clone()))
        .or(delete_sku_form(store))
}

fn index_page(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path::end()
        .and(warp::get())
        .and(cookie_filter())
        .and(store)
        .and_then(|cookie: Option<String>, store: SharedStore| async move {
            match require_admin(cookie.as_deref(), "/").await {
                AdminGate::Allow => {}
                AdminGate::SignIn(resp) => return Ok::<_, Rejection>(resp),
                AdminGate::Deny => return Err(warp::reject::not_found()),
            }
            let skus = store
                .list()
                .await
                .map_err(|e| internal_rejection("list SKUs", e))?;
            templates::render_index_html(skus, None)
                .map(|html| warp::reply::html(html).into_response())
                .map_err(|e| internal_rejection("render SKU index", e))
        })
}

fn new_sku_page(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path("skus")
        .and(warp::path("new"))
        .and(warp::path::end())
        .and(warp::get())
        .and(cookie_filter())
        .and(store)
        .and_then(|cookie: Option<String>, store: SharedStore| async move {
            match require_admin(cookie.as_deref(), "/skus/new").await {
                AdminGate::Allow => {}
                AdminGate::SignIn(resp) => return Ok::<_, Rejection>(resp),
                AdminGate::Deny => return Err(warp::reject::not_found()),
            }
            // The form offers every other SKU as a component, so the list is
            // needed even when creating.
            let skus = store
                .list()
                .await
                .map_err(|e| internal_rejection("list SKUs", e))?;
            templates::render_form_html(skus, None, None)
                .map(|html| warp::reply::html(html).into_response())
                .map_err(|e| internal_rejection("render SKU form", e))
        })
}

fn create_sku_form(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path("skus")
        .and(warp::path::end())
        .and(warp::post())
        .and(cookie_filter())
        .and(warp::body::form())
        .and(store)
        .and_then(
            |cookie: Option<String>, pairs: Vec<(String, String)>, store: SharedStore| async move {
                match require_admin(cookie.as_deref(), "/skus/new").await {
                    AdminGate::Allow => {}
                    AdminGate::SignIn(resp) => return Ok::<_, Rejection>(resp),
                    AdminGate::Deny => return Err(warp::reject::not_found()),
                }
                let form = SkuForm::from_pairs(&pairs);
                let skus = store
                    .list()
                    .await
                    .map_err(|e| internal_rejection("list SKUs", e))?;
                let values = form_to_values(&form);
                let response = match form.into_create() {
                    Ok(input) => match store.create(input).await {
                        Ok(_) => warp::redirect::redirect(warp::http::Uri::from_static("/"))
                            .into_response(),
                        Err(e) => render_form_error(skus, None, values, e),
                    },
                    Err(e) => render_form_error(skus, None, values, StoreError::InvalidInput(e)),
                };
                Ok::<_, Rejection>(response)
            },
        )
}

fn edit_sku_page(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path!("skus" / String / "edit")
        .and(warp::get())
        .and(cookie_filter())
        .and(store)
        .and_then(|id: String, cookie: Option<String>, store: SharedStore| async move {
            let return_path = format!("/skus/{id}/edit");
            match require_admin(cookie.as_deref(), &return_path).await {
                AdminGate::Allow => {}
                AdminGate::SignIn(resp) => return Ok::<_, Rejection>(resp),
                AdminGate::Deny => return Err(warp::reject::not_found()),
            }
            let (sku, skus) = tokio::join!(store.get(&id), store.list());
            let Some(sku) = sku.map_err(|e| internal_rejection("read SKU", e))? else {
                return Err(warp::reject::not_found());
            };
            let skus = skus.map_err(|e| internal_rejection("list SKUs", e))?;
            templates::render_form_html(skus, Some(sku), None)
                .map(|html| warp::reply::html(html).into_response())
                .map_err(|e| internal_rejection("render SKU form", e))
        })
}

fn update_sku_form(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path!("skus" / String / "edit")
        .and(warp::post())
        .and(cookie_filter())
        .and(warp::body::form())
        .and(store)
        .and_then(
            |id: String, cookie: Option<String>, pairs: Vec<(String, String)>, store: SharedStore| async move {
                let return_path = format!("/skus/{id}/edit");
                match require_admin(cookie.as_deref(), &return_path).await {
                    AdminGate::Allow => {}
                    AdminGate::SignIn(resp) => return Ok::<_, Rejection>(resp),
                    AdminGate::Deny => return Err(warp::reject::not_found()),
                }
                let form = SkuForm::from_pairs(&pairs);
                // Every error path re-renders the edit form, which needs both
                // the SKU list and the SKU itself: fetch them once up front.
                let (skus, sku) = tokio::join!(store.list(), store.get(&id));
                let skus = skus.map_err(|e| internal_rejection("list SKUs", e))?;
                let sku = sku.ok().flatten();
                let values = form_to_values(&form);
                let response = match form.into_update() {
                    Ok(input) => match store.update(&id, input).await {
                        Ok(_) => warp::redirect::redirect(warp::http::Uri::from_static("/"))
                            .into_response(),
                        Err(e) => render_form_error(skus, sku, values, e),
                    },
                    Err(e) => render_form_error(skus, sku, values, StoreError::InvalidInput(e)),
                };
                Ok::<_, Rejection>(response)
            },
        )
}

fn delete_sku_form(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path!("skus" / String / "delete")
        .and(warp::post())
        .and(cookie_filter())
        .and(store)
        .and_then(|id: String, cookie: Option<String>, store: SharedStore| async move {
            match require_admin(cookie.as_deref(), "/").await {
                AdminGate::Allow => {}
                AdminGate::SignIn(resp) => return Ok::<_, Rejection>(resp),
                AdminGate::Deny => return Err(warp::reject::not_found()),
            }
            match store.delete(&id).await {
                Ok(()) => {
                    Ok(warp::redirect::redirect(warp::http::Uri::from_static("/")).into_response())
                }
                Err(StoreError::NotFound) => Err(warp::reject::not_found()),
                Err(e) => {
                    let skus = store
                        .list()
                        .await
                        .map_err(|e| internal_rejection("list SKUs", e))?;
                    templates::render_index_html(skus, Some(format!("Delete failed: {e}")))
                        .map(|html| warp::reply::html(html).into_response())
                        .map_err(|e| internal_rejection("render SKU index", e))
                }
            }
        })
}

// `form_to_values` / `render_form_error` mirror the store service's pair;
// future shared-scaffold candidates once the form-values type is generic.
fn form_to_values(form: &SkuForm) -> FormValues {
    FormValues {
        sku_code: form.sku_code.clone(),
        name: form.name.clone(),
        description: form.description.clone(),
        category: form.category.clone(),
        kind: form.kind.clone(),
        active: form.active,
        components: form.components_lenient(),
    }
}

fn render_form_error(
    skus: Vec<Sku>,
    sku: Option<Sku>,
    values: FormValues,
    err: StoreError,
) -> warp::reply::Response {
    let message = err.to_string();
    match templates::render_form_html_with_values(skus, sku, Some(message), values) {
        Ok(html) => warp::reply::with_status(warp::reply::html(html), StatusCode::BAD_REQUEST)
            .into_response(),
        Err(_) => warp::reply::with_status(warp::reply(), StatusCode::INTERNAL_SERVER_ERROR)
            .into_response(),
    }
}
