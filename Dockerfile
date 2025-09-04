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

COPY --from=builder /app/target/release/pg_guard /usr/local/bin/pg_guard

RUN useradd -r -s /bin/false pg_guard

USER pg_guard

EXPOSE 6543

CMD ["pg_guard", "--listen", "0.0.0.0:6543", "--db-url", "postgres://postgres:postgres@postgres:5432/postgres"]
