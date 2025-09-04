# pg_guard Postgres Proxy Development Commands

# Default task - shows available commands
default:
    @just --list

# Run the proxy with auto-reload using cargo-watch
run:
    cargo watch -x 'run -- --db-url postgres://postgres:postgres@localhost:5432/postgres'

# Run the proxy with custom database URL
run-with-db DB_URL:
    cargo watch -x 'run -- --db-url {{DB_URL}}'

# Run the proxy in strict mode with custom max rows
run-strict MAX_ROWS="100":
    cargo watch -x 'run -- --db-url postgres://postgres:postgres@localhost:5432/postgres --strict --max-rows {{MAX_ROWS}}'

# Build the project
build:
    cargo build

# Build for release
build-release:
    cargo build --release

# Run tests
test:
    cargo test

# Check code formatting
fmt-check:
    cargo fmt --all -- --check

# Format code
fmt:
    cargo fmt --all

# Run clippy linter
clippy:
    cargo clippy --all-targets --all-features -- -D warnings

# Run all checks (fmt, clippy, test)
check: fmt-check clippy test

# Clean build artifacts
clean:
    cargo clean

# Start Docker Compose (Postgres + proxy)
up:
    docker-compose up --build -d

# Stop Docker Compose
down:
    docker-compose down

# View Docker logs
logs:
    docker-compose logs -f
