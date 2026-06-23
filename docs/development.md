# Server Development

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

## Verification

```bash
make test
make fmt
make compose-config
```
