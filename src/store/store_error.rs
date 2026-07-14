//! [`StoreError`].

#[allow(unused_imports)]
use super::*;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("sku not found")]
    NotFound,
    #[error("sku_code already exists")]
    DuplicateSkuCode,
    #[error("sku_code is required")]
    SkuCodeRequired,
    #[error("name is required")]
    NameRequired,
    #[error("composite sku must have at least one component")]
    CompositeNeedsComponents,
    #[error("simple sku cannot have components")]
    SimpleHasComponents,
    #[error("component sku not found: {0}")]
    ComponentNotFound(String),
    #[error("component quantity must be at least 1")]
    InvalidQuantity,
    #[error("composite sku cannot contain itself")]
    SelfReference,
    #[error("composite sku would create a cycle")]
    CycleDetected,
    #[error("sku is referenced by composite sku(s): {0}")]
    ReferencedByComposite(String),
    #[error("database error: {0}")]
    Database(#[from] anyhow::Error),
    #[error("{0}")]
    InvalidInput(String),
}
impl From<sqlx::Error> for StoreError {
    fn from(err: sqlx::Error) -> Self {
        Self::Database(err.into())
    }
}
