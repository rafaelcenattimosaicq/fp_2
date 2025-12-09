FROM rust:1-bookworm AS builder
WORKDIR /app

COPY Cargo.toml ./
COPY src ./src

RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -r -u 10001 app

COPY --from=builder /app/target/release/registry /usr/local/bin/registry

USER app
ENV RUST_LOG=registry=info
EXPOSE 8088
CMD ["registry"]

