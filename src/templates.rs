use askama::Template;

use crate::model::{Sku, SkuKind, format_components_text};
use sigma_identity_nav::{AppSiteNav, render_app_site_nav};
use sigma_theme::copyright_years;
use sigma_theme::nav::{Breadcrumb, SiteHeader};

fn page_header(brand: &str) -> SiteHeader {
    SiteHeader::new(brand)
}

fn site_nav(return_path: &str) -> Result<String, askama::Error> {
    render_app_site_nav(&AppSiteNav {
        identity_base: &crate::config::identity_public_base_url(),
        app_base: &crate::config::public_base_url(),
        contact_base: &crate::config::contact_public_base_url(),
        cart_url: &crate::config::cart_public_base_url(),
        cart_count: 0,
        return_path,
        show_contact_us: false,
        leading_html: "",
    })
}

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {
    skus: Vec<SkuRow>,
    message: Option<String>,
    site_header: SiteHeader,
    site_nav: String,
    copyright_years: String,
}

#[derive(Template)]
#[template(path = "form.html")]
struct FormTemplate {
    sku: Option<Sku>,
    sku_code: String,
    name: String,
    description: String,
    category: String,
    kind_simple: bool,
    kind_composite: bool,
    active: bool,
    components: String,
    available_skus: Vec<SkuRef>,
    error: Option<String>,
    site_header: SiteHeader,
    site_nav: String,
    copyright_years: String,
}

pub struct SkuRow {
    pub sku: Sku,
    pub kind_label: String,
    pub components_summary: String,
}

pub struct SkuRef {
    pub id: String,
    pub sku_code: String,
    pub name: String,
}

pub struct FormValues {
    pub sku_code: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub kind: String,
    pub active: bool,
    pub components: String,
}

fn sku_rows(skus: Vec<Sku>) -> Vec<SkuRow> {
    let code_by_id: std::collections::HashMap<String, String> = skus
        .iter()
        .map(|s| (s.id.clone(), s.sku_code.clone()))
        .collect();

    skus.into_iter()
        .map(|sku| {
            let kind_label = match sku.kind {
                SkuKind::Simple => "Simple".to_string(),
                SkuKind::Composite => "Composite".to_string(),
            };
            let components_summary = if sku.components.is_empty() {
                String::new()
            } else {
                sku.components
                    .iter()
                    .map(|c| {
                        let code = code_by_id
                            .get(&c.sku_id)
                            .map(String::as_str)
                            .unwrap_or(&c.sku_id);
                        format!("{code} × {qty}", qty = c.quantity)
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            SkuRow {
                sku,
                kind_label,
                components_summary,
            }
        })
        .collect()
}

fn sku_refs(skus: &[Sku], exclude_id: Option<&str>) -> Vec<SkuRef> {
    skus.iter()
        .filter(|s| exclude_id != Some(s.id.as_str()))
        .map(|s| SkuRef {
            id: s.id.clone(),
            sku_code: s.sku_code.clone(),
            name: s.name.clone(),
        })
        .collect()
}

fn values_from_sku(sku: &Sku) -> FormValues {
    FormValues {
        sku_code: sku.sku_code.clone(),
        name: sku.name.clone(),
        description: sku.description.clone().unwrap_or_default(),
        category: sku.category.clone().unwrap_or_default(),
        kind: match sku.kind {
            SkuKind::Simple => "simple".to_string(),
            SkuKind::Composite => "composite".to_string(),
        },
        active: sku.active,
        components: format_components_text(&sku.components),
    }
}

fn default_form_values() -> FormValues {
    FormValues {
        sku_code: String::new(),
        name: String::new(),
        description: String::new(),
        category: String::new(),
        kind: "simple".to_string(),
        active: true,
        components: String::new(),
    }
}

fn render_form(
    all_skus: Vec<Sku>,
    sku: Option<Sku>,
    error: Option<String>,
    values: FormValues,
) -> Result<String, askama::Error> {
    let kind = values.kind.to_lowercase();
    let exclude_id = sku.as_ref().map(|s| s.id.clone());
    let return_path = sku
        .as_ref()
        .map(|entry| format!("/skus/{}/edit", entry.id))
        .unwrap_or_else(|| "/skus/new".to_string());
    let form_crumb = if sku.is_some() { "Edit SKU" } else { "New SKU" };
    FormTemplate {
        sku,
        sku_code: values.sku_code,
        name: values.name,
        description: values.description,
        category: values.category,
        kind_simple: kind == "simple",
        kind_composite: kind == "composite",
        active: values.active,
        components: values.components,
        available_skus: sku_refs(&all_skus, exclude_id.as_deref()),
        error,
        site_header: page_header("Sigma Catalog")
            .with_breadcrumb(Breadcrumb::link("/", "Catalog"))
            .with_breadcrumb(Breadcrumb::current(form_crumb)),
        site_nav: site_nav(&return_path)?,
        copyright_years: copyright_years(),
    }
    .render()
}

/// # Errors
///
/// Returns [`askama::Error`] when template rendering fails.
pub fn render_index_html(skus: Vec<Sku>, message: Option<String>) -> Result<String, askama::Error> {
    IndexTemplate {
        skus: sku_rows(skus),
        message,
        site_header: page_header("Sigma Catalog"),
        site_nav: site_nav("/")?,
        copyright_years: copyright_years(),
    }
    .render()
}

/// # Errors
///
/// Returns [`askama::Error`] when template rendering fails.
pub fn render_form_html(
    all_skus: Vec<Sku>,
    sku: Option<Sku>,
    error: Option<String>,
) -> Result<String, askama::Error> {
    let values = sku
        .as_ref()
        .map(values_from_sku)
        .unwrap_or_else(default_form_values);
    render_form(all_skus, sku, error, values)
}

/// # Errors
///
/// Returns [`askama::Error`] when template rendering fails.
pub fn render_form_html_with_values(
    all_skus: Vec<Sku>,
    sku: Option<Sku>,
    error: Option<String>,
    values: FormValues,
) -> Result<String, askama::Error> {
    render_form(all_skus, sku, error, values)
}
