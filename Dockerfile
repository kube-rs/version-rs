FROM clux/muslrust:1.78.0-nightly-2024-02-26 AS builder
COPY Cargo.* .
COPY version.rs version.rs
RUN --mount=type=cache,target=/volume/target \
    --mount=type=cache,target=/root/.cargo/registry \
    cargo build --release --bin version && \
    mv /volume/target/*-unknown-linux-musl/release/version .

FROM cgr.dev/chainguard/static
COPY --from=builder --chown=nonroot:nonroot /volume/version /app/
EXPOSE 8080
ENTRYPOINT ["/app/version"]
