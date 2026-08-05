/// Resolve the signed-in user from the identity BFF using browser session cookies.
pub(crate) async fn fetch_identity_status(
    cookie_header: Option<&str>,
) -> Option<sigma_pg::clients::session::IdentityStatus> {
    sigma_pg::clients::session::fetch_identity_status(
        &crate::config::identity_internal_base_url(),
        cookie_header,
    )
    .await
    .inspect_err(|error| tracing::warn!(?error, "fetch_identity_status failed"))
    .ok()
    .flatten()
}
