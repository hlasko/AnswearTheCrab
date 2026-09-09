#![recursion_limit = "512"]

pub mod aeo;
pub mod app;
pub mod domain;
pub mod markdown;

#[cfg(feature = "ssr")]
pub mod brief_job;
#[cfg(feature = "ssr")]
pub mod draft_job;
#[cfg(feature = "ssr")]
pub mod jobs;
#[cfg(feature = "ssr")]
pub mod providers;
#[cfg(feature = "ssr")]
pub mod research;
#[cfg(feature = "ssr")]
pub mod watch;
#[cfg(feature = "ssr")]
pub mod writer;

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    use crate::app::*;
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_body(App);
}
