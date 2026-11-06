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

## Additional whole-result replay (13:44–13:52 UTC)

All mandatory Rust commands, both-target lint gates and the full build were rerun over the committed result and passed. The first fresh-database Chromium replay then failed: Tachys reported `failed_to_cast_text_node` during cold hydration, followed by an `unreachable` WASM exception. The chart itself became ready, but navigating back to Research did not complete. This is an unresolved integration failure, not a chart-rendering success.

The harness now captures complete browser error/warning diagnostics rather than only the final panic. Further checks without production-code changes passed: five full Chromium workflows, 25 sequential cold loads, 20 concurrent cold loads (four groups of five), and another twelve full Chromium workflows. A final full WebKit workflow also passed. These runs exercise all mapped outputs but do **not** prove the intermittent hydration failure is fixed. Its precise application-level origin has not been isolated. Avoid speculative router/framework changes without a reproducible failing boundary.

The namespace correction and loading states have direct before/after evidence. Overall stability acceptance remains qualified by the intermittent cold-hydration failure, and live rollout remains pending. No claim of complete fault-free delivery is made.

## Diagnostic continuation (14:12–14:18 UTC)

No application code changed in this continuation. A full Chromium workflow passed before the harness changes. A temporary read-only browser probe then passed 80 fresh-context cold loads with WASM delays cycling through 0, 10, 50, 200 and 1000 ms. Another 30 fresh-context loads passed through a local proxy delaying HTML chunks by 80 ms. Each probe waited for chart readiness and successfully navigated back to Research. These timing variations did not reproduce the historical hydration failure and do not establish its cause or a fix.

The committed smoke harness now records console arguments, including DOM node type, markup and text, instead of relying on Playwright's text-only representation. On failure it attempts to save `console.json`, `page.html` and `page.png` in a unique `chart-smoke-failure-*` directory under `ATP_CHART_ARTIFACT_DIR`, falling back to `JCODE_SCRATCH_DIR`. Without either variable, it prints the collected diagnostics. Collection has bounded waits, asynchronous failures have rejection handlers, and cleanup preserves the original test failure. If navigation destroys the execution context, HTML capture may be unavailable while the remaining artifacts are still saved.

Known environment secrets are redacted from text artifacts, executable state is omitted from the HTML clone, and form fields are masked in screenshots. This is best-effort redaction, not a guarantee that screenshots or arbitrary application content contain no sensitive information. Keep artifacts local and use only the disposable fixture installation.

Validation after the harness change: `node --check`, `git diff --check`, and the complete real-app Chromium and WebKit workflows passed. A separate synthetic failing page verified the diagnostic path: exit code 1 was retained, console DOM arguments were serialized, a fake environment secret was redacted from text outputs, and valid JSON/HTML/PNG artifacts were written. A refused-navigation check also retained its original failure and saved the available artifacts. These injected failures validate the harness, not the unresolved application hydration issue. Rust gates were not repeated because no Rust or application assets changed.

## Requirement-level follow-through (14:18–14:20 UTC)

The original user request was recovered from the preceding session (`session_spider_1793945202731_5c644545809cf951`, 2026-11-06 13:22:54 UTC): wheel charts require a reload on first entry; show a loading spinner and reveal charts only when loaded. The short continuation request was interpreted as continuing that same work, including the subsequently discovered intermittent hydration failure. Diagnostic improvements support that goal but are not a substitute for fixing a reproducible application failure or deploying the changed application.

| Requirement or changed output | Concrete check through the application or diagnostic interface | Observed result / limit |
|---|---|---|
| Chart works on first navigation without reload | Real Research link to saved search, preserved `window.chartNavigationMarker`, all SVG node namespaces and spoke graphics bounds checked | Chromium and WebKit passed. 118 spokes rendered with nonzero bounds; marker retained. |
| Spinner while loading | Hold real WASM request, inspect visible status and `aria-busy` before releasing it | Spinner visible and busy true on the compiled SSR app in both engines. |
| Reveal only after ready | Inspect computed chart visibility and PNG disabled state before/after real WASM hydration | Hidden/disabled before release, visible/enabled after readiness in both engines. |
| Newly finished results need no reload | Complete the isolated pending fixture through PostgreSQL and await normal application polling | Chart appeared with the same document marker in both engines. |
| Retry and PNG are not broken | Corrupt SVG namespace before readiness, click real retry, then export PNG on first navigation | Finite error state, clean remount and valid PNG signature in both engines. |
| Mobile / reduced motion / empty state | Real viewport at 390px, media emulation, empty-result route | No horizontal overflow, static spinner with reduced motion, no stuck empty-result spinner. The resulting 342x545 mobile chart screenshot was also visually inspected. |
| Failure JSON, HTML and PNG from the actual application | Full compiled SSR/WASM workflow through a local pass-through proxy; change one real optional-summary HTTP response status from 500 to 503 without replacing its body or application HTML | All five UI milestones passed. Harness rejected unexpected 503 with exit 1, wrote parseable JSON containing `http-error` status 503 and original failure, real empty-result DOM containing `Chart test empty` / `Research`, and PNG with a valid signature. No unhandled rejection reported. |
| Console DOM argument serialization and text redaction | Separate controlled diagnostic page logs a DOM node containing a fake environment secret | JSON retains `nodeType`/`outerHTML`; fake secret absent from JSON, HTML and captured log. This supplementary synthetic check does not stand in for the real-app test above. |
| Preserve original failure when capture is incomplete | Navigate to a browser-refused port | Original navigation failure and nonzero exit retained; JSON/PNG saved, unavailable HTML explicitly recorded as diagnostic error. |
| Sporadic cold-hydration failure fixed | 110 varied cold-start attempts plus full real-app workflows | No recurrence, but no reproduced failing boundary or demonstrated fix. Remains open. |
| User's running installation updated | No restart of the separately owned port-2999 process | Not delivered to the running installation. Acceptance is for the isolated real build on port 3018. |

The real-app failure artifacts are local under `$JCODE_SCRATCH_DIR/chart-real-failure/run-NSiNwC/`, including `run.log` and `chart-smoke-failure-uhgQ0F/{console.json,page.html,page.png}`. This demonstrates the concrete improvement over the previous text-only console output: an integration failure now leaves the real page state and HTTP failure context available for diagnosis. It does not demonstrate that the historical hydration problem has been eliminated.

## Delivery

The existing app process on port 2999 was started from a separate session and was not stopped. Validation used separately built assets under `target/chart-site`. Rebuild/restart the normal app to serve matching new server and client assets. No production records were edited.
