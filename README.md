# VibeDB: Cursor-proof your database

**A Postgres proxy with built-in safety features and row-level damage limiting**

![Cursor Drops DB](./images/cursor.png)


VibeDB is a **fully functional** hackathon-ready Postgres proxy server that sits between your applications and your database, providing real-time query analysis and protection against dangerous operations. It implements transparent TCP proxying with PostgreSQL wire protocol support with human readable messages to help claude/cursor understand how to better make DB calls.

## Features

- **Query Interception**: Analyzes all SQL queries before they reach your database
- **Row-Level Damage Limiting**: Blocks DELETE/UPDATE operations that would affect too many rows
- **Dangerous Query Blocking**: Prevents DROP, TRUNCATE, and WHERE-less DELETE operations
- **Honeytoken Protection**: Detects and blocks access to canary tables
- **Mock Snapshots**: Logs backup points before destructive operations
- **Structured Logging**: Clear, parseable logs for monitoring and debugging
- **Human Readable Errors**: Tells claude/cursor how to change its query to pass the proxy

## Quick Start

```bash
cargo install vibedb

vibedb \
  --listen 0.0.0.0:6543 \
  --db-url postgres://postgres:postgres@localhost:5432/postgres \
  --max-rows 500 \
  --strict
```

## Safety Features

### 1. Dangerous Query Blocking

```sql
-- ❌ BLOCKED: These queries are automatically blocked
DROP TABLE users;
TRUNCATE users;
DELETE FROM users;  -- No WHERE clause
```

### 2. Row-Level Damage Limiting

```sql
-- ✅ ALLOWED: Affects few rows
DELETE FROM users WHERE id = 123;

-- ❌ BLOCKED: Would affect 1000+ rows (exceeds --max-rows limit)
DELETE FROM users WHERE created_at < '2020-01-01';
```

### 3. Honeytoken Detection

```sql
-- ❌ BLOCKED: Any access to canary tables
SELECT * FROM _vibedb_canary;
```

### 4. Snapshot Logging

Before allowing destructive operations, VibeDB logs:
```
[snapshot] Would take backup here
[ALLOW] DELETE FROM users WHERE role = 'inactive' → 12 rows
```

### Testing

Connect to the proxy using any PostgreSQL client:

```bash
psql postgres://postgres:postgres@localhost:6543/postgres
```

Try these commands:
```sql
-- Safe operations (should work)
SELECT * FROM users;
UPDATE users SET role = 'pro' WHERE id = 1;

-- Dangerous operations (should be blocked)
DROP TABLE users;
DELETE FROM users;
SELECT * FROM _vibedb_canary;
```
