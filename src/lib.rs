pub mod app;
pub mod domain;

#[cfg(feature = "ssr")]
pub mod jobs;
#[cfg(feature = "ssr")]
pub mod scraper;

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    use crate::app::*;
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_body(App);
}
