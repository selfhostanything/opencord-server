COMPOSE_FILE ?= deploy/docker-compose/compose.yaml
DATABASE_URL ?= postgres://opencord:opencord@localhost:5432/opencord?sslmode=disable

.PHONY: test fmt lint dev-deps compose-app compose-config dev-api dev-realtime dev-worker migrate

test:
	cargo test --all-targets

fmt:
	cargo fmt --all

lint:
	cargo clippy --all-targets -- -D warnings

dev-deps:
	docker compose -f $(COMPOSE_FILE) up timescaledb redis minio meilisearch mailpit

compose-app:
	docker compose -f $(COMPOSE_FILE) --profile app up

compose-config:
	docker compose -f $(COMPOSE_FILE) config

dev-api:
	OPENCORD_PUBLIC_URL=http://localhost:8080 cargo run --bin api

dev-realtime:
	cargo run --bin realtime

dev-worker:
	cargo run --bin worker

migrate:
	DATABASE_URL="$(DATABASE_URL)" cargo run --bin migrate
