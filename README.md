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
src/bin/worker.rs     worker service with health endpoint and reminder polling loop
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

TURN/coturn planning and verification are documented in
[docs/coturn.md](docs/coturn.md).

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
GET /join/{join_slug}
GET /.well-known/opencord
GET /api/version
GET /api/capabilities
POST /auth/register
POST /auth/login
POST /auth/logout
GET /me
POST /media/rooms/token
POST /voice/channels/{channel_id}/join
POST /push-tokens
GET /push-tokens
POST /organizations
GET /organizations
GET /organizations/{organization_id}
POST /organizations/{organization_id}/spaces
GET /organizations/{organization_id}/spaces
POST /organizations/{organization_id}/meetings
GET /organizations/{organization_id}/meetings
GET /meetings/{meeting_id}
PATCH /meetings/{meeting_id}
DELETE /meetings/{meeting_id}
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

## Worker

`opencord-worker` polls `meeting_reminders` and fires pending reminders whose
`scheduled_for` timestamp is due. It marks successful reminders as `sent` and
dispatcher failures as `failed`.

Local defaults:

```text
OPENCORD_WORKER_ADDR=0.0.0.0:8082
OPENCORD_REMINDER_POLL_SECONDS=30
OPENCORD_REMINDER_BATCH_SIZE=100
```

The current dispatcher logs in-app, push, and email reminder deliveries. SMTP,
mobile push, and realtime in-app delivery adapters are future integration
points behind the same worker boundary.

## Media Control

`POST /voice/channels/{channel_id}/join` is the product voice entrypoint. It
authenticates the user, requires `VIEW_CHANNEL` and `CONNECT_VOICE`, issues a
short-lived LiveKit participant token, and publishes a redacted
`voice.participant_joined` realtime event.

`POST /media/rooms/token` is the lower-level media-control boundary used by the
voice join flow. It issues a short-lived LiveKit participant token after auth,
channel membership, and media permission checks.

`GET /metrics` exposes Prometheus text metrics for process-local media
observability. Phase 03 tracks voice join successes, voice join failures by
reason, and process-known voice participant counts by channel. Keep this route
behind an internal network boundary in production.

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
