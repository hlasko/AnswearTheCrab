#![recursion_limit = "512"]

pub mod app;
pub mod domain;

#[cfg(feature = "ssr")]
pub mod brief_job;
#[cfg(feature = "ssr")]
pub mod jobs;
#[cfg(feature = "ssr")]
pub mod providers;
#[cfg(feature = "ssr")]
pub mod research;

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    use crate::app::*;
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_body(App);
}
