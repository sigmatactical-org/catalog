//! [`CatalogStore`].

use std::collections::{HashMap, HashSet};

use sqlx::{PgPool, Row};

use super::StoreError;
use crate::model::{CreateSku, Sku, SkuComponent, SkuKind, UpdateSku};

#[derive(Debug, Clone)]
pub struct CatalogStore {
    pool: PgPool,
}

impl CatalogStore {
    pub async fn connect() -> Result<Self, StoreError> {
        let pool = sigma_pg::connect_as("catalog").await?;
        Ok(Self { pool })
    }

    #[cfg(test)]
    pub async fn connect_empty() -> Result<Self, StoreError> {
        let store = Self::connect().await?;
        sigma_pg::assert_disposable_test_db(&store.pool).await;
        sqlx::query("TRUNCATE catalog.sku_components, catalog.skus")
            .execute(&store.pool)
            .await?;
        Ok(store)
    }

    #[must_use]
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn list(&self) -> Result<Vec<Sku>, StoreError> {
        let rows = sqlx::query(
            "SELECT id, sku_code, name, description, category, kind, active, updated_at \
             FROM catalog.skus ORDER BY lower(sku_code)",
        )
        .fetch_all(&self.pool)
        .await?;
        self.rows_to_skus(rows).await
    }

    pub async fn get(&self, id: &str) -> Result<Option<Sku>, StoreError> {
        let row = sqlx::query(
            "SELECT id, sku_code, name, description, category, kind, active, updated_at \
             FROM catalog.skus WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        match row {
            Some(row) => Ok(self.rows_to_skus(vec![row]).await?.into_iter().next()),
            None => Ok(None),
        }
    }

    pub async fn create(&self, input: CreateSku) -> Result<Sku, StoreError> {
        self.validate_fields(&input.sku_code, &input.name, input.kind, &input.components)?;
        if self.sku_code_exists(&input.sku_code, None).await? {
            return Err(StoreError::DuplicateSkuCode);
        }
        self.validate_components(None, input.kind, &input.components)
            .await?;
        let sku = Sku::new(input);
        let mut tx = self.pool.begin().await?;
        insert_sku(&mut tx, &sku).await?;
        replace_components(&mut tx, &sku.id, &sku.components).await?;
        tx.commit().await?;
        Ok(sku)
    }

    pub async fn update(&self, id: &str, input: UpdateSku) -> Result<Sku, StoreError> {
        // One read of the current SKU serves both the existence check and the
        // value we mutate; validation takes it as an argument.
        let mut sku = self.get(id).await?.ok_or(StoreError::NotFound)?;
        self.validate_fields(&input.sku_code, &input.name, input.kind, &input.components)?;
        if self.sku_code_exists(&input.sku_code, Some(id)).await? {
            return Err(StoreError::DuplicateSkuCode);
        }
        self.validate_components(Some(id), input.kind, &input.components)
            .await?;
        sku.apply_update(input);
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "UPDATE catalog.skus SET sku_code = $2, name = $3, description = $4, category = $5, \
             kind = $6, active = $7, updated_at = $8 WHERE id = $1",
        )
        .bind(&sku.id)
        .bind(&sku.sku_code)
        .bind(&sku.name)
        .bind(&sku.description)
        .bind(&sku.category)
        .bind(sku.kind.as_str())
        .bind(sku.active)
        .bind(sku.updated_at)
        .execute(&mut *tx)
        .await?;
        replace_components(&mut tx, &sku.id, &sku.components).await?;
        tx.commit().await?;
        Ok(sku)
    }

    pub async fn delete(&self, id: &str) -> Result<(), StoreError> {
        let referencers: Vec<String> = sqlx::query_scalar(
            "SELECT s.sku_code FROM catalog.sku_components c \
             JOIN catalog.skus s ON s.id = c.parent_sku_id \
             WHERE c.component_sku_id = $1",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await?;
        if !referencers.is_empty() {
            return Err(StoreError::ReferencedByComposite(referencers.join(", ")));
        }
        // The delete itself reports existence; no separate lookup needed.
        let result = sqlx::query("DELETE FROM catalog.skus WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        if result.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    async fn rows_to_skus(&self, rows: Vec<sqlx::postgres::PgRow>) -> Result<Vec<Sku>, StoreError> {
        if rows.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<String> = rows.iter().map(|r| r.get("id")).collect();
        let comp_rows = sqlx::query(
            "SELECT parent_sku_id, component_sku_id, quantity FROM catalog.sku_components \
             WHERE parent_sku_id = ANY($1)",
        )
        .bind(&ids)
        .fetch_all(&self.pool)
        .await?;
        let mut components: HashMap<String, Vec<SkuComponent>> = HashMap::new();
        for row in comp_rows {
            let parent: String = row.get("parent_sku_id");
            components.entry(parent).or_default().push(SkuComponent {
                sku_id: row.get("component_sku_id"),
                quantity: row.get::<i32, _>("quantity") as u32,
            });
        }
        rows.into_iter()
            .map(|row| {
                let id: String = row.get("id");
                row_to_sku(row, components.remove(&id).unwrap_or_default())
            })
            .collect()
    }

    fn validate_fields(
        &self,
        sku_code: &str,
        name: &str,
        kind: SkuKind,
        components: &[SkuComponent],
    ) -> Result<(), StoreError> {
        if sku_code.trim().is_empty() {
            return Err(StoreError::SkuCodeRequired);
        }
        if name.trim().is_empty() {
            return Err(StoreError::NameRequired);
        }
        match kind {
            SkuKind::Simple if !components.is_empty() => Err(StoreError::SimpleHasComponents),
            SkuKind::Composite if components.is_empty() => {
                Err(StoreError::CompositeNeedsComponents)
            }
            _ => Ok(()),
        }
    }

    async fn sku_code_exists(
        &self,
        sku_code: &str,
        except_id: Option<&str>,
    ) -> Result<bool, StoreError> {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(
                SELECT 1 FROM catalog.skus
                WHERE lower(sku_code) = lower($1)
                  AND ($2::text IS NULL OR id <> $2)
             )",
        )
        .bind(sku_code.trim())
        .bind(except_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(exists)
    }

    async fn validate_components(
        &self,
        self_id: Option<&str>,
        kind: SkuKind,
        components: &[SkuComponent],
    ) -> Result<(), StoreError> {
        if kind != SkuKind::Composite {
            return Ok(());
        }
        for component in components {
            if component.quantity == 0 {
                return Err(StoreError::InvalidQuantity);
            }
            if self_id == Some(component.sku_id.as_str()) {
                return Err(StoreError::SelfReference);
            }
        }
        // One round trip for every component id instead of an EXISTS per row.
        let ids: Vec<String> = components.iter().map(|c| c.sku_id.clone()).collect();
        let found: Vec<String> =
            sqlx::query_scalar("SELECT id FROM catalog.skus WHERE id = ANY($1)")
                .bind(&ids)
                .fetch_all(&self.pool)
                .await?;
        let found: HashSet<String> = found.into_iter().collect();
        if let Some(missing) = ids.iter().find(|id| !found.contains(*id)) {
            return Err(StoreError::ComponentNotFound(missing.clone()));
        }
        if let Some(id) = self_id
            && self.would_cycle(id, components).await?
        {
            return Err(StoreError::CycleDetected);
        }
        Ok(())
    }

    async fn would_cycle(
        &self,
        root_id: &str,
        components: &[SkuComponent],
    ) -> Result<bool, StoreError> {
        let graph = self.component_graph().await?;
        let mut visited = HashSet::new();
        for component in components {
            if dfs_contains(&graph, &component.sku_id, root_id, &mut visited) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    async fn component_graph(&self) -> Result<HashMap<String, Vec<String>>, StoreError> {
        let rows =
            sqlx::query("SELECT parent_sku_id, component_sku_id FROM catalog.sku_components")
                .fetch_all(&self.pool)
                .await?;
        let mut graph: HashMap<String, Vec<String>> = HashMap::new();
        for row in rows {
            graph
                .entry(row.get("parent_sku_id"))
                .or_default()
                .push(row.get("component_sku_id"));
        }
        Ok(graph)
    }
}

async fn insert_sku(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    sku: &Sku,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO catalog.skus (id, sku_code, name, description, category, kind, active, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(&sku.id)
    .bind(&sku.sku_code)
    .bind(&sku.name)
    .bind(&sku.description)
    .bind(&sku.category)
    .bind(sku.kind.as_str())
    .bind(sku.active)
    .bind(sku.updated_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn replace_components(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    sku_id: &str,
    components: &[SkuComponent],
) -> Result<(), StoreError> {
    sqlx::query("DELETE FROM catalog.sku_components WHERE parent_sku_id = $1")
        .bind(sku_id)
        .execute(&mut **tx)
        .await?;
    if components.is_empty() {
        return Ok(());
    }
    // Single batched insert (one round trip) instead of a statement per row.
    let component_ids: Vec<String> = components.iter().map(|c| c.sku_id.clone()).collect();
    let quantities: Vec<i32> = components.iter().map(|c| c.quantity as i32).collect();
    sqlx::query(
        "INSERT INTO catalog.sku_components (parent_sku_id, component_sku_id, quantity) \
         SELECT $1, component_sku_id, quantity \
         FROM UNNEST($2::text[], $3::int[]) AS t(component_sku_id, quantity)",
    )
    .bind(sku_id)
    .bind(&component_ids)
    .bind(&quantities)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn row_to_sku(
    row: sqlx::postgres::PgRow,
    components: Vec<SkuComponent>,
) -> Result<Sku, StoreError> {
    let kind: String = row.get("kind");
    Ok(Sku {
        id: row.get("id"),
        sku_code: row.get("sku_code"),
        name: row.get("name"),
        description: row.get("description"),
        category: row.get("category"),
        kind: kind.parse().map_err(StoreError::InvalidInput)?,
        active: row.get("active"),
        components,
        updated_at: row.get("updated_at"),
    })
}

fn dfs_contains(
    graph: &HashMap<String, Vec<String>>,
    current: &str,
    target: &str,
    visited: &mut HashSet<String>,
) -> bool {
    if current == target {
        return true;
    }
    if !visited.insert(current.to_string()) {
        return false;
    }
    if let Some(children) = graph.get(current) {
        for child in children {
            if dfs_contains(graph, child, target, visited) {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_store() -> CatalogStore {
        CatalogStore::connect_empty()
            .await
            .expect("PostgreSQL required for tests")
    }

    fn simple(sku_code: &str, name: &str) -> CreateSku {
        CreateSku {
            sku_code: sku_code.to_string(),
            name: name.to_string(),
            description: None,
            category: None,
            kind: SkuKind::Simple,
            active: Some(true),
            components: vec![],
        }
    }

    #[tokio::test]
    async fn create_simple_sku() {
        let store = test_store().await;
        let sku = store
            .create(CreateSku {
                category: Some("parts".to_string()),
                ..simple("WIDGET-01", "Widget")
            })
            .await
            .unwrap();
        assert_eq!(sku.sku_code, "WIDGET-01");
        assert_eq!(sku.kind, SkuKind::Simple);
    }

    #[tokio::test]
    async fn create_composite_sku() {
        let store = test_store().await;
        let part = store.create(simple("PART-A", "Part A")).await.unwrap();
        let kit = store
            .create(CreateSku {
                sku_code: "KIT-01".to_string(),
                name: "Kit".to_string(),
                description: None,
                category: None,
                kind: SkuKind::Composite,
                active: Some(true),
                components: vec![SkuComponent {
                    sku_id: part.id.clone(),
                    quantity: 2,
                }],
            })
            .await
            .unwrap();
        assert_eq!(kit.components.len(), 1);
        assert_eq!(kit.components[0].quantity, 2);
    }

    #[tokio::test]
    async fn reject_unknown_component() {
        let store = test_store().await;
        let err = store
            .create(CreateSku {
                sku_code: "KIT-03".to_string(),
                name: "Kit 3".to_string(),
                description: None,
                category: None,
                kind: SkuKind::Composite,
                active: Some(true),
                components: vec![SkuComponent {
                    sku_id: "does-not-exist".to_string(),
                    quantity: 1,
                }],
            })
            .await
            .unwrap_err();
        assert!(matches!(err, StoreError::ComponentNotFound(id) if id == "does-not-exist"));
    }

    #[tokio::test]
    async fn delete_missing_sku_is_not_found() {
        let store = test_store().await;
        let err = store.delete("does-not-exist").await.unwrap_err();
        assert!(matches!(err, StoreError::NotFound));
    }

    #[tokio::test]
    async fn reject_composite_self_reference() {
        let store = test_store().await;
        let part = store.create(simple("PART-B", "Part B")).await.unwrap();
        let kit = store
            .create(CreateSku {
                sku_code: "KIT-02".to_string(),
                name: "Kit 2".to_string(),
                description: None,
                category: None,
                kind: SkuKind::Composite,
                active: Some(true),
                components: vec![SkuComponent {
                    sku_id: part.id,
                    quantity: 1,
                }],
            })
            .await
            .unwrap();
        let err = store
            .update(
                &kit.id,
                UpdateSku {
                    sku_code: kit.sku_code,
                    name: kit.name,
                    description: None,
                    category: None,
                    kind: SkuKind::Composite,
                    active: true,
                    components: vec![SkuComponent {
                        sku_id: kit.id.clone(),
                        quantity: 1,
                    }],
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, StoreError::SelfReference));
    }
}
