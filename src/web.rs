use std::convert::Infallible;

use warp::http::StatusCode;
use warp::{Filter, Rejection, Reply};

use crate::SharedStore;
use crate::model::SkuForm;
use crate::store::StoreError;
use crate::templates::{self, FormValues};

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
        .and(store)
        .and_then(|store: SharedStore| async move {
            let skus = store.list().await.map_err(|_| warp::reject::not_found())?;
            templates::render_index_html(skus, None)
                .map(warp::reply::html)
                .map_err(|_| warp::reject::not_found())
        })
}

fn new_sku_page(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path("skus")
        .and(warp::path("new"))
        .and(warp::path::end())
        .and(warp::get())
        .and(store)
        .and_then(|store: SharedStore| async move {
            let skus = store.list().await.map_err(|_| warp::reject::not_found())?;
            templates::render_form_html(skus, None, None)
                .map(warp::reply::html)
                .map_err(|_| warp::reject::not_found())
        })
}

fn create_sku_form(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path("skus")
        .and(warp::path::end())
        .and(warp::post())
        .and(warp::body::form())
        .and(store)
        .and_then(|form: SkuForm, store: SharedStore| async move {
            let skus = store.list().await.map_err(|_| warp::reject::not_found())?;
            let values = form_to_values(&form);
            let response = match form.into_create() {
                Ok(input) => match store.create(input).await {
                    Ok(_) => {
                        warp::redirect::redirect(warp::http::Uri::from_static("/")).into_response()
                    }
                    Err(e) => render_form_error(skus, None, values, e),
                },
                Err(e) => render_form_error(skus, None, values, invalid_input(e)),
            };
            Ok::<_, Rejection>(response)
        })
}

fn edit_sku_page(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path!("skus" / String / "edit")
        .and(warp::get())
        .and(store)
        .and_then(|id: String, store: SharedStore| async move {
            let Some(sku) = store
                .get(&id)
                .await
                .map_err(|_| warp::reject::not_found())?
            else {
                return Err(warp::reject::not_found());
            };
            let skus = store.list().await.map_err(|_| warp::reject::not_found())?;
            templates::render_form_html(skus, Some(sku), None)
                .map(warp::reply::html)
                .map_err(|_| warp::reject::not_found())
        })
}

fn update_sku_form(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path!("skus" / String / "edit")
        .and(warp::post())
        .and(warp::body::form())
        .and(store)
        .and_then(|id: String, form: SkuForm, store: SharedStore| async move {
            let skus = store.list().await.map_err(|_| warp::reject::not_found())?;
            let values = form_to_values(&form);
            let response = match form.into_update() {
                Ok(input) => match store.update(&id, input).await {
                    Ok(_) => {
                        warp::redirect::redirect(warp::http::Uri::from_static("/")).into_response()
                    }
                    Err(e) => {
                        let sku = store
                            .get(&id)
                            .await
                            .map_err(|_| warp::reject::not_found())?;
                        render_form_error(skus, sku, values, e)
                    }
                },
                Err(e) => {
                    let sku = store
                        .get(&id)
                        .await
                        .map_err(|_| warp::reject::not_found())?;
                    render_form_error(skus, sku, values, invalid_input(e))
                }
            };
            Ok::<_, Rejection>(response)
        })
}

fn delete_sku_form(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path!("skus" / String / "delete")
        .and(warp::post())
        .and(store)
        .and_then(|id: String, store: SharedStore| async move {
            match store.delete(&id).await {
                Ok(()) => {
                    Ok(warp::redirect::redirect(warp::http::Uri::from_static("/")).into_response())
                }
                Err(StoreError::NotFound) => Err(warp::reject::not_found()),
                Err(e) => {
                    let skus = store.list().await.map_err(|_| warp::reject::not_found())?;
                    templates::render_index_html(skus, Some(format!("Delete failed: {e}")))
                        .map(|html| warp::reply::html(html).into_response())
                        .map_err(|_| warp::reject::not_found())
                }
            }
        })
}

fn form_to_values(form: &SkuForm) -> FormValues {
    FormValues {
        sku_code: form.sku_code.clone(),
        name: form.name.clone(),
        description: form.description.clone(),
        category: form.category.clone(),
        kind: form.kind.clone(),
        active: form.active.is_some(),
        components: form.components.clone(),
    }
}

fn invalid_input(message: String) -> StoreError {
    StoreError::InvalidInput(message)
}

fn render_form_error(
    skus: Vec<crate::model::Sku>,
    sku: Option<crate::model::Sku>,
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
