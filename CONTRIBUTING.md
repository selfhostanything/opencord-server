# Contributing to OpenCord Server

OpenCord Server uses test-driven development. Add or update a focused failing
test first, implement the change, then run the relevant focused test before the
full validation suite.

## Contributor License Agreement

Human contributors must sign the [OpenCord Server Contributor License Agreement](CLA.md)
before a pull request can be merged.

When the CLA Assistant comments on your pull request, reply exactly:

```text
I have read the OpenCord Contributor License Agreement and I hereby sign the CLA
```

The workflow stores signatures at `signatures/v1/cla.json` on the default
branch. Do not create or edit that file manually.

If your employer or another organization owns your contribution, make sure you
have permission to contribute it under the CLA before opening the pull request.

## Local Validation

Run these before opening a pull request:

```bash
cargo fmt --all -- --check
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
docker compose -f deploy/docker-compose/compose.yaml config
```
