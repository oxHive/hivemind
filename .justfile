mod dashboard

import 'recipes/release.just'

_default:
  @just --choose

test:
  cargo test

build:
  cargo build

install:
  cargo install --path . --force

up:
  just dashboard build
  cargo run -- up

# Build, install, and register with an AI client for local testing (default: claude)
mcp-install client='claude': install
  hivemind mcp install {{client}}

# Force re-register by removing the existing entry first, then reinstalling
mcp-reinstall client='claude': install
  -claude mcp remove {{client}} 2>/dev/null
  hivemind mcp install {{client}}

release-major:
  just _release major

release-minor:
  just _release minor

release-patch:
  just _release patch
