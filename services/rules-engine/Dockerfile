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

COPY --from=builder /app/target/release/rules-engine /usr/local/bin/rules-engine
COPY --from=builder /app/target/release/mqtt_to_tcp_bridge /usr/local/bin/mqtt_to_tcp_bridge

USER app
ENV RUST_LOG=rules_engine=info
CMD ["rules-engine"]
