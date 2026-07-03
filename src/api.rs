use std::convert::Infallible;

use warp::http::StatusCode;
use warp::reply::Response;
use warp::{Filter, Rejection, Reply};

use crate::SharedStore;
use crate::model::{CreateSku, UpdateSku};
use crate::store::StoreError;

#[derive(serde::Serialize)]
struct ErrorBody {
    error: String,
}

fn json_error(status: StatusCode, message: impl Into<String>) -> Response {
    warp::reply::with_status(
        warp::reply::json(&ErrorBody {
            error: message.into(),
        }),
        status,
    )
    .into_response()
}

fn store_error_status(err: &StoreError) -> StatusCode {
    match err {
        StoreError::NotFound => StatusCode::NOT_FOUND,
        StoreError::DuplicateSkuCode
        | StoreError::SkuCodeRequired
        | StoreError::NameRequired
        | StoreError::CompositeNeedsComponents
        | StoreError::SimpleHasComponents
        | StoreError::ComponentNotFound(_)
        | StoreError::InvalidQuantity
        | StoreError::SelfReference
        | StoreError::CycleDetected => StatusCode::BAD_REQUEST,
        StoreError::ReferencedByComposite(_) => StatusCode::CONFLICT,
        StoreError::InvalidInput(_) => StatusCode::BAD_REQUEST,
        StoreError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

pub fn routes(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    list_skus(store.clone())
        .or(get_sku(store.clone()))
        .or(create_sku(store.clone()))
        .or(update_sku(store.clone()))
        .or(delete_sku(store))
}

fn list_skus(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path("skus")
        .and(warp::path::end())
        .and(warp::get())
        .and(store)
        .and_then(|store: SharedStore| async move {
            let skus = store.lock().await.list();
            Ok::<_, Rejection>(warp::reply::json(&skus))
        })
}

fn get_sku(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path!("skus" / String)
        .and(warp::path::end())
        .and(warp::get())
        .and(store)
        .and_then(|id: String, store: SharedStore| async move {
            let store = store.lock().await;
            match store.get(&id) {
                Some(sku) => Ok(warp::reply::json(&sku)),
                None => Err(warp::reject::not_found()),
            }
        })
}

fn create_sku(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path("skus")
        .and(warp::path::end())
        .and(warp::post())
        .and(warp::body::json())
        .and(store)
        .and_then(|input: CreateSku, store: SharedStore| async move {
            let mut store = store.lock().await;
            let response = match store.create(input).await {
                Ok(sku) => warp::reply::with_status(warp::reply::json(&sku), StatusCode::CREATED)
                    .into_response(),
                Err(e) => json_error(store_error_status(&e), e.to_string()),
            };
            Ok::<_, Rejection>(response)
        })
}

fn update_sku(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path!("skus" / String)
        .and(warp::path::end())
        .and(warp::put())
        .and(warp::body::json())
        .and(store)
        .and_then(
            |id: String, input: UpdateSku, store: SharedStore| async move {
                let mut store = store.lock().await;
                let response = match store.update(&id, input).await {
                    Ok(sku) => warp::reply::json(&sku).into_response(),
                    Err(StoreError::NotFound) => return Err(warp::reject::not_found()),
                    Err(e) => json_error(store_error_status(&e), e.to_string()),
                };
                Ok(response)
            },
        )
}

fn delete_sku(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path!("skus" / String)
        .and(warp::path::end())
        .and(warp::delete())
        .and(store)
        .and_then(|id: String, store: SharedStore| async move {
            let mut store = store.lock().await;
            let response = match store.delete(&id).await {
                Ok(()) => {
                    warp::reply::with_status(warp::reply(), StatusCode::NO_CONTENT).into_response()
                }
                Err(StoreError::NotFound) => return Err(warp::reject::not_found()),
                Err(e) => json_error(store_error_status(&e), e.to_string()),
            };
            Ok(response)
        })
}
