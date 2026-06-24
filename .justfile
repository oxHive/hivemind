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

# Release a new version: just release patch|minor|major
release bump:
  cargo release --execute {{bump}}
