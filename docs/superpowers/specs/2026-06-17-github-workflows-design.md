---
title: GitHub Workflows — CI and Cargo Publish
date: 2026-06-17
status: approved
---

## Overview

Two GitHub Actions workflows for the `oxhivemind` crate: a PR check workflow and a tag-triggered publish workflow. Both share a reusable called workflow to ensure publish never skips the same checks that CI runs.

## File Structure

```
.github/workflows/
  build.yml     — reusable called workflow (lint + test + coverage)
  ci.yml        — PR trigger → calls build.yml
  publish.yml   — v* tag trigger → calls build.yml, then cargo publish
```

## `build.yml` — Reusable Workflow

- Trigger: `on: workflow_call`
- Runner: `ubuntu-latest`
- Steps:
  1. `actions/checkout`
  2. Install stable Rust toolchain with `rustfmt` and `clippy` components
  3. `Swatinem/rust-cache` — caches `~/.cargo/registry`, `~/.cargo/git`, `./target`; keyed on OS + toolchain + `Cargo.lock`
  4. `cargo fmt --check` — fail fast on formatting violations
  5. `cargo clippy -- -D warnings` — warnings treated as errors
  6. `cargo test` — all unit and integration tests
  7. `cargo install cargo-tarpaulin --locked`
  8. `cargo tarpaulin --out Xml --fail-under 60` — fail if coverage < 60%

No system dependencies required: `rusqlite` uses the `bundled` feature.

## `ci.yml` — PR Checks

- Trigger: `on: pull_request` targeting `main`
- Jobs: single job calling `build.yml` via `uses: ./.github/workflows/build.yml`
- No secrets required

## `publish.yml` — Crates.io Publish

- Trigger: `on: push` to tags matching `v*`
- Jobs:
  1. `build` — calls `build.yml`; publish is blocked until this passes
  2. `publish` — `needs: build`; runs `cargo publish` with `CARGO_REGISTRY_TOKEN` from repo secrets

### Prerequisites

- `Cargo.toml` version must already match the pushed tag before pushing (no auto-bump)
- Secret `CARGO_REGISTRY_TOKEN` must be set in repo Settings → Secrets → Actions

## Constraints

- Coverage threshold: 60% (current codebase is at ~82%, so this is a floor not a ceiling)
- Clippy: `-D warnings` — any new warning blocks merge
- No `rustfmt` auto-fix in CI — format locally before pushing
