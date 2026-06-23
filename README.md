# OpenCord Server

OpenCord Server is the source-available backend for OpenCord: a self-hostable,
Discord-like workspace chat platform for organizations that want control over
their chat data and deployment model.

It powers OpenCord clients with multi-tenant chat, realtime messaging,
meetings, media control, bot integrations, webhooks, enterprise identity, audit,
retention, and cloud tenant operations.

## What It Provides

- Organizations, spaces, channels, roles, permissions, and audit events.
- Realtime chat with messages, attachments, embeds, replies, mentions, and
  push-token registration.
- Voice and screen-share control through a LiveKit-compatible media plane.
- Meetings, reminders, invite URLs, ICS files, and calendar sync adapters.
- Bot applications, incoming webhooks, and a Discord-compatible migration API
  for common bot and webhook workflows.
- Enterprise identity and compliance primitives: OIDC, SCIM, data export,
  retention, custom domains, usage, request IDs, and metrics.
- Cloud operations hooks for tenant provisioning and billing-provider events.

## Design

OpenCord Server starts as a Rust modular monolith. The codebase keeps HTTP
controllers, request/response models, domain logic, repository adapters, and
migrations separated without introducing microservice overhead too early.

Core stack:

- Rust, Axum, Tokio
- SeaORM migrations
- TimescaleDB/PostgreSQL 18
- Valkey-compatible cache
- Apache Kafka-compatible queue and event backbone
- ScyllaDB high-write non-ACID event/ephemeral store
- S3-compatible object storage
- Prometheus metrics with optional OTEL-ready configuration

## Quick Start

```bash
make dev-deps
make migrate
make seed
make dev-api
```

The API defaults to `http://localhost:8080`. The local seed creates
`owner@opencord.local` with password `correct horse battery staple` plus a
demo organization, channels, messages, meeting, bot, and webhook for local
alpha testing.

## Repository

```text
src/bin        API, realtime, worker, and migration entrypoints
src/routes.rs  Axum route composition
src/controllers HTTP controllers
src/domain     Domain services and authorization logic
src/events     Event envelopes and Kafka topic names
src/queue      Kafka producer/consumer boundary
src/jobs       Worker retry, idempotency, and dead-letter boundary
src/scylla     High-write ScyllaDB reference stores
src/repositories In-memory and Postgres adapters
src/db/migrations SeaORM migrations
openapi/       OpenAPI contract
docs/          Development and operations notes
```

Developer setup and verification live in [docs/development.md](docs/development.md).
TURN/coturn media notes live in [docs/coturn.md](docs/coturn.md).

## License

Elastic License 2.0
