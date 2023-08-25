[private]
default:
  @just --list --unsorted

run:
  RUST_LOG=info,kube=debug,version=debug cargo run

fmt:
  cargo +nightly fmt

build:
  docker build -t ghcr.io/kube-rs/version-rs:local .

[private]
release:
  cargo release patch --execute

[private]
import:
  k3d image import ghcr.io/kube-rs/version-rs:local --cluster main
  sd "image: .*" "image: ghcr.io/kube-rs/version-rs:local" deployment.yaml
  kubectl apply -f deployment.yaml
