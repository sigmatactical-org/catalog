use sigma_pg::clients::http::env_url;

/// Public base URL of this catalog service (e.g. `http://127.0.0.1:8080/`).
#[must_use]
pub fn public_base_url() -> String {
    env_url("CATALOG_PUBLIC_BASE_URL", "http://127.0.0.1:8080/")
}

/// Public base URL of the identity BFF (e.g. `http://127.0.0.1:3000/`).
#[must_use]
pub fn identity_public_base_url() -> String {
    env_url("CATALOG_IDENTITY_PUBLIC_URL", "http://127.0.0.1:3000/")
}

/// Browser origin of the identity BFF for CSP `connect-src` (no trailing slash).
#[must_use]
pub fn identity_public_origin() -> String {
    identity_public_base_url().trim_end_matches('/').to_string()
}

/// Public base URL of the contact service for navbar links.
#[must_use]
pub fn contact_public_base_url() -> String {
    env_url("CATALOG_CONTACT_PUBLIC_URL", "http://127.0.0.1:8083/")
}

/// Public base URL of the cart service for navbar links.
#[must_use]
pub fn cart_public_base_url() -> String {
    env_url("CATALOG_CART_PUBLIC_URL", "http://127.0.0.1:8084/")
}
