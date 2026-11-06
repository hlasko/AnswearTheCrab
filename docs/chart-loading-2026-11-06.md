# First-navigation wheel rendering, 2026-11-06

## Reproduction and fix

On the existing application, Chromium navigation from Research to a saved search produced HTML `STYLE`, `A.spoke` and `TITLE` nodes inside the SVG. Reloading the same page produced SVG-only descendants. Four affected wheels contained 91, 94, 6 and 118 spokes. This explains why reload repaired the display: HTML parsing corrected the namespaces that client-side construction got wrong.

`Wheel` now constructs ambiguous tags with explicit `leptos::svg` builders, compatible with the project's pinned macro version. `CategoryBlock` keeps chart geometry in layout but hides it until mounted SVG nodes have the correct namespaces and nonzero layout dimensions after two animation frames. Loading has a status spinner and `aria-busy`, PNG is disabled until ready, and a failed readiness check exposes a retry that remounts the SVG. Disposed components are not updated by late frame callbacks. Search resource loading also has a spinner. Reduced-motion users see a static indicator.

## Acceptance observations

`scripts/chart-smoke.mjs` ran against the actual compiled SSR/WASM app with real migrations in a disposable local PostgreSQL database. No rendering code or server function was mocked. Provider credentials were disabled, and no harvest jobs were queued.

| Requirement / output | Check | Observed result |
|---|---|---|
| Spinner before ready | Hold the real WASM request on a cold load | Spinner visible, `aria-busy=true`, SVG hidden and PNG disabled. Release WASM: chart visible, busy false, PNG enabled. |
| No reload needed | Click actual Research result link, retain a window marker | Marker preserved. 118 spokes have SVG-only namespaces and nonzero graphics bounds. |
| Newly completed search | Open pending search, finish its fixture data in DB, let normal polling run | Wheel appears without reloading the document. |
| Error is not endless loading | Inject an HTML node before readiness | Error shown, busy false, PNG disabled. Retry remounts clean SVG and becomes ready. |
| Export still works | Click PNG after first SPA navigation | Valid PNG signature, 589718 bytes in Chromium and 540580 in WebKit. |
| Mobile / motion / empty | 390px viewport, reduced motion, empty finished search | No horizontal overflow, spinner animation disabled, no charts or stuck spinner on empty results. |
| Navigation stability | Final sequential runs: Chromium twice, WebKit once | All scenarios pass, no page/hydration exceptions. |

The isolated app automatically requests optional summaries. Its expected `500 /api/summarise_section` responses contain exactly `summaries need OPENROUTER_API_KEY`; the test checks that exception explicitly and rejects other HTTP failures. An earlier test with artificial API delay stalled on cancelled requests, so final SPA checks use unmodified networking. One earlier browser run reported an `unreachable` page error without a diagnostic stack; it did not recur in the final three runs and is not claimed as a separately diagnosed/fixed bug. A no-JavaScript test was not accepted: the application's existing streamed Suspense response needs JavaScript to insert results. This task does not claim no-JavaScript support.

## Rust gates

Passed after the code changes: `cargo check`, `cargo clippy -- -D warnings`, `cargo test` (135 passed, 17 optional tests ignored), `cargo clippy --all-targets -- -D warnings`, hydrate-target Clippy with `-D warnings`, and `cargo leptos build`. The existing dependency future-compatibility advisory remains.

For repeatable browser testing, start an isolated application with an empty `atp_chart_*` database, real migrations and provider credentials disabled, then run:

```sh
ATP_SMOKE_ALLOW_FIXTURES=1 \
ATP_CHART_TEST_DATABASE_URL=postgresql:///atp_chart_test \
ATP_CHART_BASE_URL=http://127.0.0.1:3018 \
node scripts/chart-smoke.mjs
```

Set `ATP_CHART_BROWSER=webkit` to repeat with WebKit. The script resets only its fixed fixture search suggestions inside the explicitly named disposable database.

## Delivery

The existing app process on port 2999 was started from a separate session and was not stopped. Validation used separately built assets under `target/chart-site`. Rebuild/restart the normal app to serve matching new server and client assets. No production records were edited.
