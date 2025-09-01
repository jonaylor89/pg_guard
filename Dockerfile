FROM rust:1.87 as builder

WORKDIR /app

COPY Cargo.toml ./

COPY src/ ./src/

RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/vibedb /usr/local/bin/vibedb

RUN useradd -r -s /bin/false vibedb

USER vibedb

EXPOSE 6543

CMD ["vibedb", "--listen", "0.0.0.0:6543", "--db-url", "postgres://postgres:postgres@postgres:5432/postgres"]
