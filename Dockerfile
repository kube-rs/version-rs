FROM rust:1.72-alpine as builder
RUN apk add --no-cache musl-dev
WORKDIR /repo

# Cache downloaded + built dependencies
COPY Cargo.toml Cargo.lock /repo/
RUN echo 'fn main() {}' > /repo/version.rs && \
    cargo build --release && \
    rm -f /repo/version.rs

# Build our code
COPY version.rs /repo/
RUN cargo build --release

# Runtime
FROM cgr.dev/chainguard/static
EXPOSE 8080
COPY --from=builder --chown=nonroot:nonroot /repo/target/release/version /
ENTRYPOINT ["/version"]
