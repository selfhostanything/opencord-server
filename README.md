# OpenCord Server

Source-available backend services for OpenCord.

## Stack

- Rust
- Axum HTTP framework
- SeaORM and SeaORM migrations
- Tokio async runtime
- TimescaleDB/PostgreSQL 18 local database

## Structure

```text
src/bin/api.rs        REST API, health, discovery
src/bin/realtime.rs   realtime service placeholder with health endpoint
src/bin/worker.rs     worker service placeholder with health endpoint
src/bin/migrate.rs    SeaORM migration runner
src/controllers       HTTP controllers
src/models            response/request DTOs
src/routes.rs         Axum route composition
src/domain            auth service and domain helpers such as UUIDv7 IDs
src/repositories      in-memory and Postgres persistence adapters
src/db/migrations     SeaORM migrations
```

The server stays a modular monolith first. The route/controller/model/domain split is intentionally simple so Phase 01 can add auth, organization, channel, message, and permission modules without creating microservice overhead.

## Local Development

Start dependencies:

```bash
make dev-deps
```

Run tests:

```bash
make test
```

Run the API locally:

```bash
make dev-api
```

Run the full backend stack in Docker Compose:

```bash
make compose-app
```

## Database

Local development uses:

```text
timescale/timescaledb:2.28.0-pg18
```

Run migrations:

```bash
make migrate
```

## Endpoints

```text
GET /healthz
GET /.well-known/opencord
GET /api/version
GET /api/capabilities
POST /auth/register
POST /auth/login
POST /auth/logout
GET /me
```

Auth endpoints use bearer session tokens:

```text
Authorization: Bearer <session token>
```

## License

OpenCord Server License 1.0 placeholder. Legal review required before production release.
