
VERSION := `git rev-parse HEAD`

[private]
default:
  @just --list --unsorted

run:
  RUST_LOG=info,kube=debug,version=debug cargo run

fmt:
  cargo +nightly fmt

build:
  docker build -t clux/version:{{VERSION}} .
