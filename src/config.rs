//! Environment-driven configuration (service URLs, database URL).
//!
//! Required values are declared in the [`sigma_config::service!`] block and
//! checked by [`validate_with`] at startup.

sigma_config::service! {
    prefix = "CATALOG";
    role = "catalog";
    urls {
        /// Public base URL of this catalog service.
        public_base_url = "PUBLIC_BASE_URL" => "http://127.0.0.1:8080/";
        /// Public base URL of the identity BFF.
        identity_public_base_url = "IDENTITY_PUBLIC_URL" => "http://127.0.0.1:3000/";
        /// Public base URL of the contact service, for navbar links.
        contact_public_base_url = "CONTACT_PUBLIC_URL" => "http://127.0.0.1:8083/";
        /// Public base URL of the cart service, for navbar links.
        cart_public_base_url = "CART_PUBLIC_URL" => "http://127.0.0.1:8084/";
    }
}

/// Browser origin of the identity BFF for CSP `connect-src` (no trailing slash).
#[must_use]
pub fn identity_public_origin() -> String {
    sigma_config::origin_of(&identity_public_base_url())
}

/// Base URL for server-to-server calls to the identity BFF (e.g. session
/// status checks on HTML admin routes). Must be reachable from this pod,
/// unlike `identity_public_base_url`, which is the browser-facing ingress
/// host and does not resolve back to identity from inside the cluster
/// network. Falls back to the public URL for non-cluster local dev.
#[must_use]
pub fn identity_internal_base_url() -> String {
    SERVICE
        .opt_url("IDENTITY_INTERNAL_URL")
        .unwrap_or_else(identity_public_base_url)
}

/// PostgreSQL connection URL (shared Sigma database).
#[must_use]
pub fn database_url() -> String {
    SERVICE.database_url()
}
