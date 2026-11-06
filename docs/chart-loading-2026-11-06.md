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

## Remaining-hypothesis experiments (14:20–14:27 UTC)

The original failed run's saved log was recovered. Its browser stack stopped after ten frames inside Rust/Tachys panic handling, so it could not identify the application component. The smoke browser now installs `Error.stackTraceLimit = 100` before loading the app. This is test-only and changes no production assets.

| Hypothesis / diagnostic requirement | Experiment | Observation |
|---|---|---|
| The order of nested SSR resources causes the mismatch | 21 cold navigations with temporary transaction-scoped exclusive locks on the disposable database's `watches`, `summaries`, `youtube_checks`, `youtube_appetite`, `ai_answer_runs`, `prompt_selections` or `suggestions` table, each held for 0.25, 1.5 or 4 seconds | All chart-ready and Research-return checks passed without page exceptions. Locks were rolled back. This matrix changes database availability but does not prove every listed table is read on this route. |
| It occurs only on the first request after migrations / cold server state | Six independently created empty databases, six new application processes on port 3020, real startup migrations, then the entire smoke workflow immediately after startup | All six workflows passed. Each owned process stopped and its successful disposable database was removed. Existing ports 2999/3018 were untouched. |
| Navigation disposes pending work before hydration/readiness completes | 25 real Research-to-result-to-Research round trips with API requests delayed 400 ms; return after 0, 30, 100, 450 or 800 ms, followed by a final uninterrupted result navigation | Every round trip preserved the document marker, no page exception occurred, and the final chart became ready. |
| The new diagnostic output still omits the failing application location | A local proxy injected a deliberate DOM text-to-element substitution in `.result-head .provider` before hydrating the actual compiled application | The harness failed as expected and recorded `failed_to_cast_text_node`, the unexpected DOM node, location `src/app.rs:2557:26`, and a 101-line page-error stack. This is a controlled fault validating diagnostics, not reproduction of the unexplained original failure. |

After the deeper-stack change, the full Chromium and WebKit smoke workflows, `node --check` and `git diff --check` passed again.

Detailed local evidence: `$JCODE_SCRATCH_DIR/chart-cold-server/` contains each fresh-server/workflow log, `chart-navigation-probe/result.json` contains navigation diagnostics, and `chart-real-hydration-fault/run-2qsjce/` contains the controlled hydration failure and its artifacts. The additional tests do not justify a speculative framework/router change. The unexplained historical failure and live rollout remain open; repeated passing runs are not presented as their resolution.

## Smoke-harness review and saturation regression (14:27–14:32 UTC)

Comparing against `dc5f46f` confirmed that all 21 prior assertion lines, fixture SQL and chart-readiness checks remain intact. Four unsafe fixture configurations were also executed and rejected before SQL/browser startup: missing opt-in, the normal `atp` database, a nonmatching URL form and a URL with query options.

The review found a real defect in the new diagnostics: its first-300-entry policy allowed ordinary console logs to fill the buffer before a hydration error. This was reproduced on the compiled app by emitting 310 ordinary logs immediately before the existing controlled text-node hydration fault. Before the fix, the artifact lacked `pageerror` and the regression failed at `assert.ok(pageerror)`.

`scripts/chart-diagnostics.mjs` now implements a bounded priority buffer. Final failure, page/HTTP errors, error/warning console entries and diagnostic errors take precedence over ordinary logs. At capacity, the oldest entry in the lowest eligible tier is replaced; lower-priority arrivals cannot evict higher-priority evidence. Original entry references are retained for asynchronous DOM-argument capture. Ordinary log entries keep text without scheduling DOM serialization.

| Changed behavior | Verification | Result |
|---|---|---|
| Preserve late hydration evidence after log saturation | Repeat the same compiled-app 310-log fault injection | Passed: at most 300 entries, final failure retained, `pageerror` with 101 stack lines, `failed_to_cast_text_node` and DOM arguments present. Artifacts: `$JCODE_SCRATCH_DIR/chart-real-hydration-fault/run-9h17NJ/`. |
| Priority, boundedness and async enrichment | `node --test scripts/chart-diagnostics.test.mjs` | Six tests passed, including all priority pairs, earlier/later noise, FIFO among peers, reference identity and invalid capacity rejection. |
| Original user acceptance is unchanged | Full real-app Chromium and WebKit workflows after buffer integration | Both passed loading/readiness, no-reload navigation, PNG, retry, polling, mobile, reduced-motion and empty-result assertions without hydration exceptions. |
| Syntax and patch hygiene | `node --check scripts/chart-smoke.mjs`, `git diff --check` | Passed. |

This is a demonstrated fix to diagnostic evidence retention. It is not a fix to the unexplained historical application hydration failure, which remains open.

## Repeatable diagnostic-buffer workflow

The complete reproducer is now checked in as `scripts/chart-diagnostics-integration.mjs`, rather than depending on a temporary scratch script. It requires an already running isolated compiled application connected to the explicitly named disposable database, with paid provider credentials disabled. The script does not start, rebuild or stop that application.

From the repository root:

```sh
npm --prefix scripts run test:chart-diagnostics

ATP_SMOKE_ALLOW_FIXTURES=1 \
ATP_CHART_TEST_DATABASE_URL=postgresql:///atp_chart_test \
ATP_CHART_BASE_URL=http://127.0.0.1:3018 \
ATP_CHART_ARTIFACT_DIR="$PWD/target/chart-artifacts" \
npm --prefix scripts run test:chart-diagnostics:integration
```

The integration command first requires the ordinary Chromium smoke workflow to pass. It then uses a temporary loopback proxy to inject log saturation and a controlled text-node hydration mismatch into that same real application. Success requires the smoke child to fail as expected, while retaining the bounded diagnostic buffer, deep stack, unexpected DOM node, HTML and valid PNG. A passing integration command therefore does not mean the injected smoke run passed; it means the failure was correctly detected and preserved. Both runs have separate logs and artifact directories. The proxy is stopped on completion.

Use `npm --prefix scripts run test:chart-smoke` with the same fixture variables for the ordinary workflow alone; add `ATP_CHART_BROWSER=webkit` for WebKit. Integration fault injection is intentionally Chromium-specific because its deep-stack assertion uses Chromium's stack-limit behavior. No new package dependencies are required.

Observed end-to-end on 2026-11-06 (14:34–14:41 UTC): all three npm commands passed, including the ordinary WebKit smoke invocation. The integration command passed from the repository root, and the final script passed again when invoked by absolute path from outside the repository. Both integration runs verified an unmodified baseline followed by the expected injected failure, retained evidence after 310 logs before and 310 logs after the error, and PNG chunk/checksum/decompression checks. Eight invalid configurations were rejected before creating any run directory or starting a child/proxy. Final portable-run artifacts are under `$JCODE_SCRATCH_DIR/chart-public-workflow/chart-diagnostics-integration-Vh7GNr/`. Child timeouts and proxy cleanup are part of the checked-in runner; the existing application process is never stopped by it.

## Delivery

The existing app process on port 2999 was started from a separate session and was not stopped. Validation used separately built assets under `target/chart-site`. Rebuild/restart the normal app to serve matching new server and client assets. No production records were edited.
