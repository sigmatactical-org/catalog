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

fn internal_auth(
) -> impl Filter<Extract = (Option<String>, Option<String>), Error = Rejection> + Clone {
    warp::header::optional::<String>("authorization")
        .and(warp::header::optional::<String>("x-sigma-internal-token"))
}

fn ensure_internal(
    authorization: Option<String>,
    internal_token: Option<String>,
) -> Result<(), Rejection> {
    if sigma_pg::clients::internal::authorize_internal(
        authorization.as_deref(),
        internal_token.as_deref(),
    ) {
        Ok(())
    } else {
        Err(warp::reject::not_found())
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
        .and(internal_auth())
        .and(store)
        .and_then(
            |authorization, internal_token, store: SharedStore| async move {
                ensure_internal(authorization, internal_token)?;
                let skus = store.list().await.map_err(|_| warp::reject::not_found())?;
                Ok::<_, Rejection>(warp::reply::json(&skus))
            },
        )
}

fn get_sku(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path!("skus" / String)
        .and(warp::path::end())
        .and(warp::get())
        .and(internal_auth())
        .and(store)
        .and_then(
            |id: String, authorization, internal_token, store: SharedStore| async move {
                ensure_internal(authorization, internal_token)?;
                match store
                    .get(&id)
                    .await
                    .map_err(|_| warp::reject::not_found())?
                {
                    Some(sku) => Ok(warp::reply::json(&sku)),
                    None => Err(warp::reject::not_found()),
                }
            },
        )
}

fn create_sku(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path("skus")
        .and(warp::path::end())
        .and(warp::post())
        .and(warp::body::json())
        .and(internal_auth())
        .and(store)
        .and_then(
            |input: CreateSku, authorization, internal_token, store: SharedStore| async move {
                ensure_internal(authorization, internal_token)?;
                let response = match store.create(input).await {
                    Ok(sku) => warp::reply::with_status(warp::reply::json(&sku), StatusCode::CREATED)
                        .into_response(),
                    Err(e) => json_error(store_error_status(&e), e.to_string()),
                };
                Ok::<_, Rejection>(response)
            },
        )
}

fn update_sku(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path!("skus" / String)
        .and(warp::path::end())
        .and(warp::put())
        .and(warp::body::json())
        .and(internal_auth())
        .and(store)
        .and_then(
            |id: String, input: UpdateSku, authorization, internal_token, store: SharedStore| async move {
                ensure_internal(authorization, internal_token)?;
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
        .and(internal_auth())
        .and(store)
        .and_then(
            |id: String, authorization, internal_token, store: SharedStore| async move {
                ensure_internal(authorization, internal_token)?;
                let response = match store.delete(&id).await {
                    Ok(()) => {
                        warp::reply::with_status(warp::reply(), StatusCode::NO_CONTENT).into_response()
                    }
                    Err(StoreError::NotFound) => return Err(warp::reject::not_found()),
                    Err(e) => json_error(store_error_status(&e), e.to_string()),
                };
                Ok(response)
            },
        )
}
