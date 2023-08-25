FROM rust:1.72-alpine as builder
RUN apk add --no-cache musl-dev

# Cache downloaded + built dependencies
COPY Cargo.toml Cargo.lock /
RUN echo 'fn main() {}' > /version.rs && \
    cargo build --release && \
    rm -f /version.rs

# Build our code
COPY version.rs /
RUN cargo build --release

# Runtime
FROM cgr.dev/chainguard/static
EXPOSE 8080
COPY --from=builder --chown=nonroot:nonroot /target/release/version /
ENTRYPOINT ["/version"]
