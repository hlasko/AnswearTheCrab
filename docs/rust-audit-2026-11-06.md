# Rust logic and stability audit, 2026-11-06

## Scope and acceptance

Reviewed the repository's Rust panic extractions, provider construction, prompt approval boundaries, database decoding, scheduling and numeric calculations. This is a code audit with regression tests, not a penetration test, formal proof, dependency vulnerability assessment, or certification that all possible panics have been eliminated. Test assertions deliberately still fail the test when behavior is wrong.

Required acceptance commands are `cargo check`, `cargo clippy -- -D warnings`, and `cargo test`. Final results are recorded below after integration. Additional checks cover all test targets, the hydrate build, and opt-in isolated PostgreSQL tests with a loopback-only HTTP provider. No paid provider calls are part of validation.

## Confirmed findings and changes

| Finding | Impact | Resolution / evidence |
|---|---|---|
| Due AI watches were read before requests and stamped afterward | Concurrent scheduler ticks could pay for the same approved set twice | Conditional database claim before external requests. A blocked loopback provider lets a second tick run while the first request remains in flight. |
| Keyword and page schedules also read due rows before stamping | Multiple schedulers could enqueue duplicate harvests or repeat page-provider calls | Conditional preclaims recheck eligibility in one SQL update. Tests assert one winner, skip disabled items and prevent immediate reclaims. |
| AI run headers and individual answers were inserted separately | An insert failure could leave a misleading partial run, and total provider failure could leave an empty run | Execute paid work first, reject total failure, then persist successful responses and header in one transaction. DB failure regression checks rollback. |
| Stored approval JSON was deserialized without enforcing text invariants | Directly corrupted/legacy snapshots could send blank or duplicate requests despite UI validation | Private `ApprovedPrompts` type checked on selection, save and load. Reject empty/over-20 lists, blank normalized text, semantic duplicates, and text exceeding 4096 bytes. Corrupted-storage mock regression verifies zero provider calls. |
| Approval checked search existence separately from the write | Concurrent deletion could turn the upsert into a silent zero-row insert | A short transaction holds a share lock on the search through the approval write. No provider request runs inside this transaction. |
| Arbitrary schedule intervals reached SQL | Zero/negative intervals could continuously recur; extreme values could overflow date arithmetic | Checked `DaysInterval` input, accepting 1 through 3650 days. Scheduler queries exclude invalid persisted values and CASE-bound interval arithmetic regardless of optimizer predicate ordering. |
| HTTP constructors used `expect` | Client initialization errors could terminate execution | Constructors return `Result`, propagated to startup/server/job boundaries. Optional enrichment logs initialization failure without discarding the harvest. |
| Unknown draft kind silently became an article | Malformed input could trigger unintended paid generation | Strict `FromStr` for draft kind with explicit rejection and persisted job failure. |
| Unknown search source silently became Google | A malformed submitted or queued source could trigger the wrong provider | Strict `Source::from_str` at search creation and harvest execution. Legacy read-only display keeps its original fallback. |
| Radar used infallible row extraction | Unexpected legacy column types or nulls could panic | `try_get`, fallible collection and checked count conversion return errors to the server boundary. |
| A brief held a DB transaction while awaiting answer-engine requests | Network delays unnecessarily occupied a pool connection and row locks | Complete external work before beginning the persistence transaction. |
| Large/negative provider metrics and non-finite CPC values entered arithmetic | Integer overflow, invalid ranking scores or nonsensical demand | Widen percentage arithmetic, saturate aggregate counts, exclude invalid metrics, and use checked rank differences. Regressions include empty input, zero, integer extremes, NaN and infinity. |
| Malformed approval aborted an AI scheduler tick | One corrupt watch blocked healthy watches | Skip and log the invalid watch. Mixed malformed/healthy regression verifies only the healthy prompt reaches the provider. |
| Queue push failure left a pending search | Failed delivery appeared indefinitely pending | Persist failed status and finish time while retaining the claim. Closed queue pool regression exercises this failure without enqueueing work. |
| Numerous test helpers used panic extraction | Fixture/setup failures were harder to propagate and could bypass cleanup | Fallible test functions and fixtures, retained assertions, isolated schema cleanup on errors and assertion unwinding. |

`Cargo.toml` denies Clippy's `unwrap_used`, `expect_used`, `panic`, `unreachable`, and `todo` lints. This prevents reintroducing explicit extraction/panic patterns in linted targets, but is not a proof that indexing, dependencies, allocation or framework internals cannot panic.

## Operational limits

- Claiming before execution prefers avoiding duplicate scheduled billing over guaranteed execution. A crash after a claim can consume that interval without completing its work. This is not an exactly-once payment protocol.
- Queue delivery remains uncertain if its commit succeeds before a connection error. Error handling is not an outbox or exactly-once delivery protocol.
- Explicit manual requests remain independent operations. Scheduling claims do not deduplicate separately submitted manual actions.
- Disabling a watch or changing approval does not cancel a request already started. A run uses a checked approval snapshot read before making its provider requests.
- A provider can charge for a request even if a later DB write fails. Transactional persistence cannot roll back an external charge.
- Partial provider success retains only successful answers, as before. Total failure returns an error without creating an empty run.
- The app remains single-user. Tenant isolation/authentication for shared deployment is outside this audit.
- No production data, live provider correctness, load limits, screen-reader behavior or market-level prompt relevance is certified here.

## Validation results

Final integrated source through `6d2741e` passed on 2026-11-06:

- `cargo fmt --check`
- `cargo check`
- `cargo clippy -- -D warnings`
- `cargo test`: 132 passed, 0 failed, 17 opt-in tests ignored.
- `cargo clippy --all-targets -- -D warnings`
- `PROMPT_TEST_DATABASE_URL=postgresql:///postgres cargo test --test prompt_discovery -- --ignored`: all 8 passed, covering concurrent claims, connection release during requests, transaction rollback, corrupt snapshots/intervals, healthy-watch isolation, and queue push failure.

Additional checks passed earlier in this audit:

- `RADAR_TEST_DATABASE_URL=postgresql:///postgres cargo test --test radar --test schema_ui_contract -- --ignored`: 2 passed, including real migration replay and fallible decoding.
- `PAGE_TEST_DATABASE_URL=postgresql:///postgres cargo test --lib scheduled_page_claims_are_exclusive_and_skip_disabled -- --ignored`: 1 passed.
- `cargo clippy --no-default-features --features hydrate --target wasm32-unknown-unknown -- -D warnings` and `cargo leptos build`: passed before the final SSR-only watch changes.

11 of the 17 default-ignored tests were explicitly exercised. Six real-data/optional tests were not run. PostgreSQL fixtures were isolated, HTTP providers used loopback mocks, and no paid API calls were made.

Evidence boundaries: page and keyword claim tests exercise real database helpers, not an entire production queue deployment. Approval deletion locking and shorter brief transactions were reviewed structurally, not through deletion/network fault injection. This audit did not rerun browser interactions. Passing checks does not prove the absence of every race or panic.

Cargo still reports a future-compatibility advisory for dependency `proc-macro-error2 v2.0.1`. Leptos reports an existing unrecognized metadata key `env`. Neither prevented acceptance. Earlier transient compilation/lint failures during parallel integration were fixed.
