mod component_line;
mod component_option;
mod form_template;
mod form_values;
mod index_template;
mod sku_row;
pub use component_line::ComponentLine;
pub use component_option::ComponentOption;
pub(crate) use form_template::FormTemplate;
pub use form_values::FormValues;
pub(crate) use index_template::IndexTemplate;
pub use sku_row::SkuRow;

use askama::Template;

use crate::model::{Sku, SkuComponent, SkuKind};
use sigma_theme::copyright_years;
use sigma_theme::nav::{Breadcrumb, SiteHeader, site_menu};
use sigma_theme::site_nav::{AppSiteNav, render_app_site_nav};

fn page_header() -> SiteHeader {
    SiteHeader::new("Catalog").with_menu(site_menu(None))
}

fn site_nav(return_path: &str) -> Result<String, askama::Error> {
    render_app_site_nav(&AppSiteNav {
        identity_base: &crate::config::identity_public_base_url(),
        app_base: &crate::config::public_base_url(),
        contact_base: &crate::config::contact_public_base_url(),
        cart_url: &crate::config::cart_public_base_url(),
        cart_count: 0,
        return_path,
        show_cart: true,
        show_contact_us: false,
        leading_html: "",
    })
}

fn sku_rows(skus: Vec<Sku>) -> Vec<SkuRow> {
    let by_id: std::collections::HashMap<String, (String, String)> = skus
        .iter()
        .map(|s| (s.id.clone(), (s.sku_code.clone(), s.name.clone())))
        .collect();

    skus.into_iter()
        .map(|sku| {
            let kind_label = match sku.kind {
                SkuKind::Simple => "Simple".to_string(),
                SkuKind::Composite => "Composite".to_string(),
            };
            let components = sku
                .components
                .iter()
                .map(|c| {
                    let (sku_code, name) = by_id
                        .get(&c.sku_id)
                        .cloned()
                        .unwrap_or_else(|| (c.sku_id.clone(), String::new()));
                    ComponentLine {
                        sku_code,
                        name,
                        quantity: c.quantity,
                    }
                })
                .collect();
            SkuRow {
                sku,
                kind_label,
                components,
            }
        })
        .collect()
}

/// Every SKU except the one being edited, marked with the current selection.
fn component_options(
    skus: &[Sku],
    exclude_id: Option<&str>,
    selected: &[SkuComponent],
) -> Vec<ComponentOption> {
    skus.iter()
        .filter(|s| exclude_id != Some(s.id.as_str()))
        .map(|s| {
            let selection = selected.iter().find(|c| c.sku_id == s.id);
            ComponentOption {
                id: s.id.clone(),
                sku_code: s.sku_code.clone(),
                name: s.name.clone(),
                selected: selection.is_some(),
                quantity: selection.map_or(1, |c| c.quantity),
            }
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
        components: sku.components.clone(),
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
        components: Vec::new(),
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
        component_options: component_options(&all_skus, exclude_id.as_deref(), &values.components),
        sku,
        sku_code: values.sku_code,
        name: values.name,
        description: values.description,
        category: values.category,
        kind_simple: kind == "simple",
        kind_composite: kind == "composite",
        active: values.active,
        error,
        site_header: page_header()
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
        site_header: page_header(),
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
