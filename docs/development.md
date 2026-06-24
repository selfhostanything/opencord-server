# Server Development

## Stack

- Rust
- Axum HTTP framework
- SeaORM and SeaORM migrations
- Tokio async runtime
- TimescaleDB/PostgreSQL 18 local database
- Valkey-compatible cache
- Apache Kafka-compatible event and job backbone
- ScyllaDB high-write non-ACID reference store

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
src/events            event envelopes, topic names, and idempotency keys
src/queue             queue producer/consumer contracts and consumer groups
src/jobs              worker retry and idempotency primitives
src/scylla            ScyllaDB table names and high-write store contracts
src/observability     log, trace-context, request ID, and OTEL config helpers
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

Host-run Rust services use these local dependency defaults:

```text
KAFKA_BOOTSTRAP_SERVERS=localhost:29092
SCYLLA_CONTACT_POINTS=localhost:9042
VALKEY_URL=redis://localhost:6379/0
OPENCORD_LOG_FORMAT=text
OPENCORD_OTEL_ENABLED=false
OPENCORD_ALLOWED_ORIGINS=http://localhost:5173,http://127.0.0.1:5173
```

`make dev-api`, `make dev-realtime`, and `make dev-worker` pass the local
`DATABASE_URL` into host-run Rust services. The API target also allows Vite
browser origins by default so the official web client can call auth,
organization, space, channel, message, and meeting endpoints during local
alpha testing.

Seed deterministic local-alpha data after migrations:

```bash
make seed
```

The seed command is idempotent and creates:

- owner `owner@opencord.local` with password `correct horse battery staple`
- organization `OpenCord Local Alpha`
- space `Local Alpha`
- text channel `general`
- voice channel `Voice Lounge`
- seeded rich messages and an attachment fixture
- meeting `OpenCord Local Alpha Standup`
- local bot and webhook fixtures

The command prints local-only session, bot, and webhook tokens. Treat them as
developer fixtures and regenerate by rerunning `make seed`.

Kafka and ScyllaDB architecture smokes are opt-in so the normal test loop stays
fast:

```bash
docker compose -f deploy/docker-compose/compose.yaml up -d kafka scylladb

docker compose -f deploy/docker-compose/compose.yaml exec -T kafka \
  /opt/kafka/bin/kafka-topics.sh \
  --bootstrap-server localhost:9092 \
  --create \
  --if-not-exists \
  --topic opencord.events.chat.v1 \
  --partitions 3 \
  --replication-factor 1

OPENCORD_KAFKA_SMOKE=1 \
KAFKA_BOOTSTRAP_SERVERS=localhost:29092 \
cargo test --test kafka_queue_smoke -- --nocapture

OPENCORD_SCYLLA_SMOKE=1 \
SCYLLA_CONTACT_POINTS=localhost:9042 \
cargo test --test scylla_store_smoke -- --nocapture
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
node IP: OPENCORD_LIVEKIT_NODE_IP, defaulting to 127.0.0.1
```

On macOS Docker/OrbStack, WebRTC UDP may fail if LiveKit advertises loopback
from inside the container. Set `OPENCORD_LIVEKIT_NODE_IP` to the active host
LAN address before running `make dev-media`, for example:

```bash
OPENCORD_LIVEKIT_NODE_IP="$(ipconfig getifaddr en0)" make dev-media
```

If browser media still connects to signaling but ICE never selects a working
UDP pair, run the local-only host-network LiveKit target instead:

```bash
make dev-media-hostnet
```

This uses the same pinned `livekit/livekit-server:v1.13` image with Docker host
networking and is intended for same-machine media verification only.

TURN/coturn is not required for same-machine media development. Use
[`docs/coturn.md`](coturn.md) when testing relay fallback or production media
networking.

## Verification

```bash
make fmt
make lint
make test
make compose-config
make migrate
cargo test --test architecture_foundation --test openapi_contract
```

Local alpha smoke commands:

```bash
curl -fsS http://localhost:8080/healthz
curl -fsS http://localhost:8081/healthz
curl -fsS http://localhost:8082/healthz
curl -fsS http://localhost:8080/.well-known/opencord
curl -fsS http://localhost:8080/metrics

OPENCORD_KAFKA_SMOKE=1 \
KAFKA_BOOTSTRAP_SERVERS=localhost:29092 \
cargo test --test kafka_queue_smoke -- --nocapture

OPENCORD_SCYLLA_SMOKE=1 \
SCYLLA_CONTACT_POINTS=localhost:9042 \
cargo test --test scylla_store_smoke -- --nocapture
```

For the supported HTTP contract, use `openapi/openapi.yaml` as the source of
truth. `tests/openapi_contract.rs` checks contract-critical routes locally. The
compatibility bot contract is also covered by the `compat_*` integration tests.

## Observability

The server must run without OTEL infrastructure:

```text
OPENCORD_OTEL_ENABLED=false
```

Every HTTP response includes `x-request-id`. If the client sends a non-empty
`x-request-id`, the server preserves it; otherwise the server generates a
`req_<uuidv7>` value. Prometheus `/metrics` includes HTTP request, Kafka, job,
and ScyllaDB counters.

Use JSON logs when a collector or log pipeline expects structured records:

```text
OPENCORD_LOG_FORMAT=json
OPENCORD_LOG_FILTER=opencord_server=info,info
```
