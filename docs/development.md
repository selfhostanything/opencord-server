# Server Development

## Stack

- Rust
- Axum HTTP framework
- SeaORM and SeaORM migrations
- Tokio async runtime
- TimescaleDB/PostgreSQL 18 local database
- Valkey-compatible cache

## Structure

```text
src/bin/api.rs        REST API, health, discovery
src/bin/realtime.rs   realtime service placeholder with health endpoint
src/bin/worker.rs     worker service with health endpoint and reminder polling loop
src/bin/migrate.rs    SeaORM migration runner
src/controllers       HTTP controllers
src/models            response/request DTOs
src/routes.rs         Axum route composition
src/domain            auth, permissions, media-control, bot, and domain helpers
src/repositories      in-memory and Postgres persistence adapters
src/db/migrations     SeaORM migrations
```

The server stays a modular monolith first. The route/controller/model/domain
split is intentionally simple so product capabilities can land without
microservice overhead.

## Fast Mode

```bash
make dev-deps
make migrate
make dev-api
```

## Full Local Stack

```bash
make compose-app
```

## Optional Local Media

Start only LiveKit:

```bash
make dev-media
```

Start the app profile with LiveKit:

```bash
make compose-media
```

LiveKit runs in development mode with:

```text
url: ws://localhost:7880
api key: devkey
api secret: secret
ports: 7880/tcp, 7881/tcp, 7882/udp
```

TURN/coturn is not required for same-machine media development. Use
[`docs/coturn.md`](coturn.md) when testing relay fallback or production media
networking.

## Verification

```bash
make test
make fmt
make compose-config
```

For the supported HTTP contract, use `openapi/openapi.yaml` as the source of
truth. The compatibility bot contract is also covered by the
`compat_*` integration tests.
