# Plasma

Plasma is a coding agent with an application layer, independently deployable feature packages, and foundation packages. Keep package responsibilities narrow so incremental builds stay fast.

Terminal commands and the slash-command menu must expose the same user workflows.

Treat inference providers as services Plasma uses, not as agents. Authentication needs to be host-driven so a terminal, desktop, or mobile application can choose the appropriate browser and callback flow.

## Rust module layout

Avoid monolith `main.rs` and giant modules. As a binary grows, split it into focused modules under `src/` (e.g. `app.rs`, `ui.rs`, `event.rs`, `theme.rs`) and keep `main.rs` to bootstrap, configuration parsing, and a thin entry point. The same rule applies to library crates: a single `lib.rs` over a few hundred lines is a signal to split into submodules. Prefer many small files with clear names over one large file, so reviewers can navigate and incremental rebuilds stay cheap.
