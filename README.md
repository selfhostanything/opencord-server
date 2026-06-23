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
src/domain            auth, permissions, media-control, and domain helpers
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

Run the optional local LiveKit media server:

```bash
make dev-media
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
GET /ws
GET /.well-known/opencord
GET /api/version
GET /api/capabilities
POST /auth/register
POST /auth/login
POST /auth/logout
GET /me
POST /media/rooms/token
POST /push-tokens
GET /push-tokens
POST /organizations
GET /organizations
GET /organizations/{organization_id}
POST /organizations/{organization_id}/spaces
GET /organizations/{organization_id}/spaces
POST /spaces/{space_id}/channels
GET /spaces/{space_id}/channels
POST /spaces/{space_id}/members
POST /spaces/{space_id}/roles
POST /spaces/{space_id}/roles/{role_id}/assignments
GET /spaces/{space_id}/audit-events
PATCH /channels/{channel_id}
POST /channels/{channel_id}/permission-overrides
POST /channels/{channel_id}/messages
GET /channels/{channel_id}/messages
POST /attachments/presign
PUT /attachments/{attachment_id}/content
GET /attachments/{attachment_id}/content
PATCH /messages/{message_id}
DELETE /messages/{message_id}
```

Auth endpoints use bearer session tokens:

```text
Authorization: Bearer <session token>
```

## Media Control

`POST /media/rooms/token` issues a short-lived LiveKit participant token after
auth, channel membership, and media permission checks. It currently supports
`voice_channel` rooms and prepares the service boundary for the Phase 03 voice
join flow.

Local defaults:

```text
OPENCORD_LIVEKIT_URL=ws://localhost:7880
OPENCORD_LIVEKIT_API_KEY=devkey
OPENCORD_LIVEKIT_API_SECRET=secret
OPENCORD_MEDIA_TOKEN_TTL_SECONDS=600
OPENCORD_MEDIA_REGION=local
```

Docker Compose exposes optional LiveKit development mode through the `media`
profile:

```bash
make dev-media
make compose-media
```

Ports:

```text
7880/tcp LiveKit signal/API
7881/tcp LiveKit TCP fallback
7882/udp LiveKit RTC UDP
```

## License

Elastic License 2.0 (`Elastic-2.0`).
