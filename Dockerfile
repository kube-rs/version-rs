FROM clux/muslrust:stable AS builder
COPY Cargo.* .
COPY version.rs version.rs
RUN --mount=type=cache,target=/volume/target \
    --mount=type=cache,target=/root/.cargo/registry \
    cargo install --bin version --path=.

FROM cgr.dev/chainguard/static
COPY --from=builder --chown=nonroot:nonroot /opt/cargo/bin/version /app/
EXPOSE 8080
ENTRYPOINT ["/app/version"]
