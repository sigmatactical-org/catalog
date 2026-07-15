//! [`FormTemplate`].

#[allow(unused_imports)]
use super::*;
use crate::model::Sku;
use askama::Template;
use sigma_theme::nav::SiteHeader;

#[derive(Template)]
#[template(path = "form.html")]
pub(crate) struct FormTemplate {
    pub(crate) sku: Option<Sku>,
    pub(crate) sku_code: String,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) category: String,
    pub(crate) kind_simple: bool,
    pub(crate) kind_composite: bool,
    pub(crate) active: bool,
    pub(crate) component_options: Vec<ComponentOption>,
    pub(crate) error: Option<String>,
    pub(crate) site_header: SiteHeader,
    pub(crate) site_nav: String,
    pub(crate) copyright_years: String,
}
