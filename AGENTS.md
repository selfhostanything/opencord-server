# AGENTS.md

This repository follows the OpenCord root agent rules.

Important local rules:

- Use TDD for behavior changes.
- Use `cargo test --all-targets` for focused and broad backend checks.
- Run `cargo fmt --all` before finishing Rust changes.
- Use UUIDv7 for all native UUID identifiers.
- Use SeaORM migrations for schema changes.
- Use `timescale/timescaledb:2.28.0-pg18` for local TimescaleDB/PostgreSQL.
- Docker images must use Debian 13/trixie base images.
- Keep the Rust code organized around controllers, models, routes, domain modules, and database migrations unless a future task proves another structure is better.
- Do not add Phase 01 chat features during Phase 00.
