# Media Control Service

Media-control owns LiveKit room and participant-token decisions for OpenCord.

In the modular-monolith runtime this is currently an in-process domain service
used by the API. The boundary is explicit so it can become a separate
deployment later if media scaling requires it.

## Current API Surface

```text
POST /media/rooms/token
```

The API controller:

- Authenticates the bearer session.
- Verifies the requested organization, space, and channel are consistent.
- Checks channel membership and permissions.
- Requires `CONNECT_VOICE` for any media token.
- Requires `SPEAK`, `USE_VIDEO`, or `SHARE_SCREEN` for matching publish grants.
- Calls media-control to mint a short-lived, room-scoped LiveKit token.

## Environment

```text
OPENCORD_LIVEKIT_URL=ws://localhost:7880
OPENCORD_LIVEKIT_API_KEY=devkey
OPENCORD_LIVEKIT_API_SECRET=secret
OPENCORD_MEDIA_TOKEN_TTL_SECONDS=600
OPENCORD_MEDIA_REGION=local
```

The local defaults are development-only. Production deployments must provide
unique LiveKit credentials.

## Local LiveKit

The Docker Compose `media` profile runs LiveKit in development mode:

```bash
make dev-media
```

or together with the app containers:

```bash
make compose-media
```

The Compose API container points `OPENCORD_LIVEKIT_URL` at
`ws://livekit:7880`; host development uses the default `ws://localhost:7880`.
