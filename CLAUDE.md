# ajj Development Guide

AI agents: read `.claude/skills/ajj-guide.md` for a comprehensive crate
overview before making changes.

## Crate Summary

`ajj` is a JSON-RPC 2.0 router library. Single crate (not a workspace). Core
pipeline: Handler -> Method -> Router -> Server. Feature-gated for axum, ws,
and ipc transports. Built on tower and serde_json.

## Commands

- `cargo +nightly fmt`
- `cargo clippy -p ajj --all-features --all-targets`
- `cargo clippy -p ajj --no-default-features --all-targets`
- `cargo t -p ajj`

Always lint with both `--all-features` and `--no-default-features` before
committing. Never use `cargo check` or `cargo build`.

## Repo Conventions

- Feature gates: `axum`, `pubsub`, `ws`, `ipc`. Default: `axum`, `ws`, `ipc`.
  `ws` and `ipc` both imply `pubsub`.
- `Router<S>` wraps `Arc<RouterInner<S>>`. `S` is "missing" state, not
  current state. After `with_state()`, `S` becomes the next missing type.
- Handlers are type-erased via `ErasedIntoRoute` and stored in
  `BTreeMap<MethodId, Method<S>>`.
- Internal macros `tap_inner!`, `map_inner!`, `panic_on_err!`,
  `impl_handler_call!` are used extensively in router and handler code.
- OpenTelemetry spans follow OTEL semantic conventions for JSON-RPC.
- Code provenance: some code adapted from axum and alloy. Files containing
  adapted code have license notices at the bottom.

## init4 Organization Style (summary of ../CLAUDE.md)

### Research

- Prefer building crate docs (`cargo doc`) and reading them over grepping.

### Code Style

- Functional combinators over imperative control flow. No unnecessary nesting.
- Terse Option/Result handling: `option.map(Thing::do_something)` or
  `let Some(a) = option else { return; };`.
- Small, focused functions and types.
- Never add incomplete code. No `TODO`s for core logic.
- Never use glob imports. Group imports from the same crate. No blank lines
  between imports.
- Visibility: private by default, `pub(crate)` for internal, `pub` for API.
  Never use `pub(super)`.

### Error Handling

- `thiserror` for library errors. Never `anyhow`. Never `eyre` in libraries.
- Propagate with `?` and `map_err`.

### Tracing

- Use `tracing` crate. Instrument work items, not long-lived tasks.
- `skip(self)` when instrumenting methods. Add only needed fields.
- Levels: TRACE (rare, verbose), DEBUG (sparingly), INFO (default),
  WARN (potential issues), ERROR (prevents operation).
- Propagate spans through task boundaries with `Instrument`.

### Async

- Tokio runtime conventions. No blocking in async functions.
- Long-lived tasks: return a spawnable future, don't run directly.
- Short-lived spawned tasks: consider span propagation.

### Testing

- Tests panic, never return `Result`. Use `unwrap()` directly.
- Unit tests in `mod tests` at file bottom. Integration tests in `tests/`.

### Rustdoc

- Doc all public items. Include usage examples in rustdoc.
- Hide scaffolding with `#`. Keep examples concise.
- Traits must include an implementation guide.

### GitHub

- Fresh branches off `main` for PRs. Descriptive branch names.
- AI-authored GitHub comments must include `**[Claude Code]**` header.
