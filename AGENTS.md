# AGENTS.md

This repository follows the OpenCord root agent rules.

Important local rules:

- Use TDD for behavior changes.
- Use `cargo test --all-targets` for focused and broad backend checks.
- Run `cargo fmt --all` before finishing Rust changes.
- Use UUIDv7 for all native UUID identifiers.
- Use SeaORM migrations for schema changes.
- Use `timescale/timescaledb:2.28.1-pg18-oss` for local TimescaleDB/PostgreSQL.
- Do not use Docker `latest` tags. Prefer major.minor tags that float patch
  updates, for example `valkey/valkey:8.1-alpine`; when an image only publishes
  release/date tags, use the current explicit release tag.
- Docker images must use Debian 13/trixie base images.
- Keep the Rust code organized around controllers, models, routes, domain modules, and database migrations unless a future task proves another structure is better.
- Do not add Phase 01 chat features during Phase 00.
