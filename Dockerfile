FROM clux/muslrust:stable AS builder
COPY Cargo.* .
COPY version.rs version.rs
RUN --mount=type=cache,target=/volume/target \
    --mount=type=cache,target=/root/.cargo/registry \
    cargo build --release --bin version && \
    mv /volume/target/x86_64-unknown-linux-musl/release/version .

FROM gcr.io/distroless/static:nonroot
COPY --from=builder --chown=nonroot:nonroot /volume/version /app/
EXPOSE 8080
ENTRYPOINT ["/app/version"]
