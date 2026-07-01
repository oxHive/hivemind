_default:
  @just --choose

test:
  cargo test

[working-directory: 'dashboard']
dashboard:
  bun run build

build:
  cargo build

install:
  cargo install --path . --force

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

# Release a new version: just release patch|minor|major
_release bump:
  cargo release --execute {{bump}}
