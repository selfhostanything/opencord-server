# coturn and TURN Planning

OpenCord uses LiveKit for WebRTC media and coturn for TURN relay fallback.
TURN is optional for local same-machine development, but it is required for
reliable company deployments because many users sit behind restrictive NATs,
VPNs, and office firewalls.

## Local Development

For normal host-local development:

```bash
make dev-media
```

LiveKit dev mode is enough to test token issuance and same-machine media
connections. Add coturn locally only when testing relay fallback.

Suggested local coturn command:

```bash
docker run --rm --name opencord-coturn \
  -p 3478:3478/tcp \
  -p 3478:3478/udp \
  -p 49160-49200:49160-49200/udp \
  coturn/coturn:latest \
  -n \
  --log-file=stdout \
  --lt-cred-mech \
  --user opencord:opencord-turn-password \
  --realm opencord.local \
  --listening-port=3478 \
  --min-port=49160 \
  --max-port=49200 \
  --fingerprint \
  --no-multicast-peers
```

Local ICE server values:

```text
OPENCORD_TURN_URLS=turn:localhost:3478?transport=udp,turn:localhost:3478?transport=tcp
OPENCORD_TURN_USERNAME=opencord
OPENCORD_TURN_CREDENTIAL=opencord-turn-password
```

## Production Requirements

Production TURN should be reachable through a stable DNS name such as:

```text
turn.chat.example.com
```

Minimum network openings:

```text
3478/udp        TURN over UDP
3478/tcp        TURN over TCP fallback
5349/tcp        TURN over TLS when enabled
50000-60000/udp Relay allocation range
```

For LiveKit VM-style deployments that terminate TURN/TLS on the primary HTTPS
port, also allow:

```text
443/tcp         HTTPS and TURN/TLS
80/tcp          ACME HTTP challenge when using automatic certificates
7881/tcp        LiveKit WebRTC over TCP
```

On Vultr, configure these rules in the cloud firewall and the instance firewall
if one is enabled. DNS for the primary LiveKit host and TURN host must point to
the media node before certificate issuance.

## Security Rules

Required:

- Do not run anonymous TURN in production.
- Use long-term credentials or TURN REST API shared-secret credentials.
- Rotate TURN shared secrets.
- Keep relay port ranges intentionally bounded.
- Keep coturn logs available for media incident review.
- Rate-limit and monitor allocation failures.
- Do not enable loopback peers in production.

Recommended production environment:

```text
OPENCORD_TURN_URLS=turn:turn.chat.example.com:3478?transport=udp,turn:turn.chat.example.com:3478?transport=tcp,turns:turn.chat.example.com:5349?transport=tcp
OPENCORD_TURN_SHARED_SECRET=<secret>
OPENCORD_TURN_REALM=chat.example.com
```

## Helm Values Plan

The chart repo should support both external and bundled coturn.

```yaml
turn:
  enabled: true
  mode: external # external | bundled
  urls:
    - turn:turn.chat.example.com:3478?transport=udp
    - turn:turn.chat.example.com:3478?transport=tcp
    - turns:turn.chat.example.com:5349?transport=tcp
  existingSecret: opencord-turn
  realm: chat.example.com
  relayPortRange:
    min: 50000
    max: 60000
  service:
    type: LoadBalancer
    annotations: {}
  resources: {}
  networkPolicy:
    enabled: true
```

Secret keys:

```text
TURN_SHARED_SECRET
TURN_USERNAME
TURN_CREDENTIAL
```

Bundled mode is for evaluation and small installs. Production should prefer a
dedicated coturn deployment or the LiveKit-generated VM configuration with
reviewed firewall rules and TLS.

## Verification Checklist

Before calling a media environment ready:

1. DNS resolves for LiveKit and TURN hosts.
2. TLS certificates are valid.
3. UDP relay ports are reachable from outside the private network.
4. A browser client can join through normal UDP.
5. A browser client can join with UDP blocked and fall back to TURN/TCP or TURN/TLS.
6. Media-control still issues room-scoped tokens only after OpenCord permission checks.
