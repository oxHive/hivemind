test:
  cargo test

build:
  cargo build

install:
  cargo install --path . --force

run arg:
  just dashboard build
  cargo run -- {{ arg }}
