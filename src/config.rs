/// PostgreSQL connection URL (shared Sigma database).
#[must_use]
pub fn database_url() -> String {
    std::env::var("DATABASE_URL").unwrap_or_else(|_| sigma_pg::service_database_url("catalog"))
}
