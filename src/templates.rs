use handlebars::{handlebars_helper, Handlebars};
use rust_embed::RustEmbed;
use serde_json::Value;

use crate::library::format_size;
use crate::paths;

#[derive(RustEmbed)]
#[folder = "views/"]
#[include = "*.hbs"]
struct Views;

#[derive(RustEmbed)]
#[folder = "static/"]
pub struct Static;

handlebars_helper!(format_size_helper: |bytes: u64| format_size(bytes));
handlebars_helper!(url_path: |path: str| paths::url_encode(path));

pub fn registry() -> Handlebars<'static> {
    let mut hbs = Handlebars::new();
    hbs.set_strict_mode(false);
    hbs.register_helper("formatSize", Box::new(format_size_helper));
    hbs.register_helper("urlPath", Box::new(url_path));
    hbs.register_embed_templates_with_extension::<Views>(".hbs")
        .expect("templates are valid");
    hbs
}

/// Renders `view` inside the main layout.
pub fn render_page(hbs: &Handlebars, view: &str, data: Value) -> String {
    render_with_layout(hbs, view, "layouts/main", data)
}

/// Renders `view` inside `layout`, which receives the view's HTML as `body`.
pub fn render_with_layout(hbs: &Handlebars, view: &str, layout: &str, mut data: Value) -> String {
    let render = |name: &str, data: &Value| {
        hbs.render(name, data).unwrap_or_else(|err| {
            tracing::error!("template {name}: {err}");
            String::from("Template error")
        })
    };
    let body = render(view, &data);
    if let Value::Object(map) = &mut data {
        map.insert("body".into(), Value::String(body));
    }
    render(layout, &data)
}
