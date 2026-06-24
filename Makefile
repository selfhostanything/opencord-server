COMPOSE_FILE ?= deploy/docker-compose/compose.yaml
DATABASE_URL ?= postgres://opencord:opencord@localhost:5432/opencord?sslmode=disable
OPENCORD_DEV_ALLOWED_ORIGINS ?= http://localhost:5173,http://127.0.0.1:5173
OPENCORD_LIVEKIT_NODE_IP ?= $(shell ipconfig getifaddr en0 2>/dev/null || echo 127.0.0.1)
OPENCORD_TURN_HOST_IP ?= $(shell ipconfig getifaddr en0 2>/dev/null || echo 127.0.0.1)
OPENCORD_TURN_MIN_PORT ?= 49160
OPENCORD_TURN_MAX_PORT ?= 49200

.PHONY: test fmt lint dev-deps dev-turn dev-media dev-media-turn dev-media-hostnet dev-media-turn-hostnet compose-app compose-media compose-config dev-api dev-realtime dev-worker migrate seed

test:
	cargo test --all-targets

fmt:
	cargo fmt --all

lint:
	cargo clippy --all-targets -- -D warnings

dev-deps:
	docker compose -f $(COMPOSE_FILE) up timescaledb valkey kafka scylladb minio meilisearch mailpit

dev-turn:
	docker rm -f opencord-coturn >/dev/null 2>&1 || true
	docker run --rm --name opencord-coturn -p 3478:3478/tcp -p 3478:3478/udp -p $(OPENCORD_TURN_MIN_PORT)-$(OPENCORD_TURN_MAX_PORT):$(OPENCORD_TURN_MIN_PORT)-$(OPENCORD_TURN_MAX_PORT)/udp coturn/coturn:4.14-debian -n --log-file=stdout --lt-cred-mech --user opencord:opencord-turn-password --realm opencord.local --listening-ip=0.0.0.0 --relay-ip=0.0.0.0 --external-ip=$(OPENCORD_TURN_HOST_IP) --listening-port=3478 --min-port=$(OPENCORD_TURN_MIN_PORT) --max-port=$(OPENCORD_TURN_MAX_PORT) --fingerprint --allowed-peer-ip=10.0.0.0-10.255.255.255 --allowed-peer-ip=172.16.0.0-172.31.255.255 --allowed-peer-ip=192.168.0.0-192.168.255.255 --allow-loopback-peers --server-relay --no-multicast-peers

dev-media:
	docker compose -f $(COMPOSE_FILE) --profile media up livekit

dev-media-turn:
	docker compose -f $(COMPOSE_FILE) --profile turn up coturn livekit-turn

dev-media-hostnet:
	docker rm -f opencord-livekit-hostnet >/dev/null 2>&1 || true
	docker run --rm --name opencord-livekit-hostnet --network host livekit/livekit-server:v1.13 --dev --bind 0.0.0.0 --node-ip $(OPENCORD_LIVEKIT_NODE_IP) --rtc.enable_loopback_candidate

dev-media-turn-hostnet:
	docker rm -f opencord-livekit-hostnet >/dev/null 2>&1 || true
	docker run --rm --name opencord-livekit-hostnet --network host -v $(CURDIR)/deploy/livekit/livekit-turn.yaml:/etc/livekit/livekit-turn.yaml:ro livekit/livekit-server:v1.13 --config /etc/livekit/livekit-turn.yaml --node-ip $(OPENCORD_TURN_HOST_IP)

compose-app:
	docker compose -f $(COMPOSE_FILE) --profile app up

compose-media:
	docker compose -f $(COMPOSE_FILE) --profile app --profile media up

compose-config:
	docker compose -f $(COMPOSE_FILE) config

dev-api:
	DATABASE_URL="$(DATABASE_URL)" OPENCORD_PUBLIC_URL=http://localhost:8080 OPENCORD_ALLOWED_ORIGINS="$(OPENCORD_DEV_ALLOWED_ORIGINS)" cargo run --bin api

dev-realtime:
	DATABASE_URL="$(DATABASE_URL)" OPENCORD_PUBLIC_URL=http://localhost:8080 cargo run --bin realtime

dev-worker:
	DATABASE_URL="$(DATABASE_URL)" OPENCORD_PUBLIC_URL=http://localhost:8080 cargo run --bin worker

migrate:
	DATABASE_URL="$(DATABASE_URL)" cargo run --bin migrate

seed:
	DATABASE_URL="$(DATABASE_URL)" OPENCORD_PUBLIC_URL=http://localhost:8080 cargo run --bin seed
