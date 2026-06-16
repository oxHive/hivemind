_default:
  @just --choose

test:
    cargo test

[working-directory: 'dashboard']
dashboard:
    bun run build

build:
    cargo build
