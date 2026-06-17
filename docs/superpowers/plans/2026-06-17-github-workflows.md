# GitHub Workflows Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add three GitHub Actions workflow files — a reusable build/check workflow, a PR CI workflow, and a tag-triggered crates.io publish workflow — plus fix a stale `dependabot.yml` entry.

**Architecture:** `build.yml` is a `workflow_call`-triggered reusable workflow that runs lint, test, and coverage. `ci.yml` invokes it on every PR. `publish.yml` invokes it on `v*` tag pushes and, if it passes, runs `cargo publish`. No code changes to the Rust crate itself.

**Tech Stack:** GitHub Actions, `dtolnay/rust-toolchain`, `Swatinem/rust-cache`, `cargo-tarpaulin`

## Global Constraints

- Runner: `ubuntu-latest` for all jobs
- Rust toolchain: `stable` channel
- Coverage threshold: `--fail-under 60`
- Clippy: `-D warnings` (warnings are errors)
- Tarpaulin installed with `--locked` to pin to `Cargo.lock` version
- Publish secret name: `CARGO_REGISTRY_TOKEN`
- No `Co-Authored-By` lines in commit messages

---

### Task 1: Fix `dependabot.yml` and create `build.yml`

**Files:**
- Modify: `.github/dependabot.yml`
- Create: `.github/workflows/build.yml`

**Interfaces:**
- Produces: reusable workflow callable via `uses: ./.github/workflows/build.yml` with no inputs

- [ ] **Step 1: Fix the stale `gomod` ecosystem entry in dependabot.yml**

The current file incorrectly lists `gomod` — this is a Rust project. Replace the full file:

```yaml
version: 2

updates:
  - package-ecosystem: cargo
    directory: /
    schedule:
      interval: weekly
    labels:
      - dependencies
    commit-message:
      prefix: "chore(deps)"

  - package-ecosystem: github-actions
    directory: /
    schedule:
      interval: weekly
    labels:
      - dependencies
    commit-message:
      prefix: "chore(ci)"
```

- [ ] **Step 2: Create `.github/workflows/` directory and `build.yml`**

```bash
mkdir -p .github/workflows
```

Create `.github/workflows/build.yml`:

```yaml
name: Build & Check

on:
  workflow_call:

jobs:
  build:
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v4

      - name: Install Rust stable
        uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy

      - name: Cache Cargo
        uses: Swatinem/rust-cache@v2

      - name: Check formatting
        run: cargo fmt --check

      - name: Clippy
        run: cargo clippy -- -D warnings

      - name: Run tests
        run: cargo test

      - name: Install cargo-tarpaulin
        run: cargo install cargo-tarpaulin --locked

      - name: Check coverage
        run: cargo tarpaulin --out Xml --fail-under 60
```

- [ ] **Step 3: Validate YAML syntax**

```bash
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/build.yml'))" && echo "OK"
python3 -c "import yaml; yaml.safe_load(open('.github/dependabot.yml'))" && echo "OK"
```

Expected: both print `OK` with no errors.

- [ ] **Step 4: Commit**

```bash
git add .github/dependabot.yml .github/workflows/build.yml
git commit -m "ci: add reusable build workflow and fix dependabot ecosystem"
```

---

### Task 2: Create `ci.yml` — PR check workflow

**Files:**
- Create: `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: `.github/workflows/build.yml` (from Task 1) via `uses: ./.github/workflows/build.yml`

- [ ] **Step 1: Create `.github/workflows/ci.yml`**

```yaml
name: CI

on:
  pull_request:
    branches:
      - main

jobs:
  ci:
    uses: ./.github/workflows/build.yml
```

- [ ] **Step 2: Validate YAML syntax**

```bash
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))" && echo "OK"
```

Expected: prints `OK`.

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: add PR check workflow"
```

---

### Task 3: Create `publish.yml` — crates.io publish on tag push

**Files:**
- Create: `.github/workflows/publish.yml`

**Interfaces:**
- Consumes: `.github/workflows/build.yml` (from Task 1) via `uses: ./.github/workflows/build.yml`
- Requires: repo secret `CARGO_REGISTRY_TOKEN` (set manually in GitHub repo Settings → Secrets → Actions)

- [ ] **Step 1: Create `.github/workflows/publish.yml`**

```yaml
name: Publish to crates.io

on:
  push:
    tags:
      - 'v*'

jobs:
  build:
    uses: ./.github/workflows/build.yml

  publish:
    needs: build
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v4

      - name: Install Rust stable
        uses: dtolnay/rust-toolchain@stable

      - name: Cache Cargo
        uses: Swatinem/rust-cache@v2

      - name: Publish to crates.io
        run: cargo publish
        env:
          CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}
```

- [ ] **Step 2: Validate YAML syntax**

```bash
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/publish.yml'))" && echo "OK"
```

Expected: prints `OK`.

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/publish.yml
git commit -m "ci: add crates.io publish workflow on v* tag push"
```

---

## Post-Implementation Checklist

- [ ] Set `CARGO_REGISTRY_TOKEN` secret in GitHub repo Settings → Secrets → Actions (login to crates.io → Account Settings → API Tokens to generate)
- [ ] Push a test PR branch to verify the CI workflow triggers and passes
- [ ] To publish: bump version in `Cargo.toml`, commit, tag (`git tag v0.1.0`), push tag (`git push origin v0.1.0`)
