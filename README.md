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
timescale/timescaledb:2.28.1-pg18-oss
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
POST /api/webhooks/{webhook_id}/{webhook_token}
GET /api/compat/discord/v10/users/@me
GET /api/compat/discord/v10/guilds/{space_id}
GET /api/compat/discord/v10/guilds/{space_id}/channels
GET /api/compat/discord/v10/guilds/{space_id}/roles
POST /api/compat/discord/v10/channels/{channel_id}/messages
GET /api/compat/discord/v10/channels/{channel_id}/messages
PATCH /api/compat/discord/v10/channels/{channel_id}/messages/{message_id}
DELETE /api/compat/discord/v10/channels/{channel_id}/messages/{message_id}
POST /api/compat/discord/v10/applications/{application_id}/guilds/{space_id}/commands
POST /api/compat/discord/v10/interactions/{interaction_id}/{interaction_token}/callback
POST /api/compat/discord/v10/webhooks/{application_id}/{interaction_token}
PATCH /api/compat/discord/v10/webhooks/{application_id}/{interaction_token}/messages/@original
GET /api/compat/discord/gateway
POST /auth/register
POST /auth/login
GET /auth/oidc/providers
POST /auth/oidc/callback
POST /auth/logout
GET /me
POST /billing/provider-events
POST /cloud/tenants
GET /calendar/accounts
POST /calendar/accounts/caldav
POST /calendar/accounts/google
POST /calendar/accounts/microsoft
POST /media/rooms/token
POST /voice/channels/{channel_id}/join
POST /push-tokens
GET /push-tokens
POST /organizations
GET /organizations
GET /organizations/{organization_id}
GET /organizations/{organization_id}/usage
GET /organizations/{organization_id}/audit-events/export
GET /organizations/{organization_id}/data-export
GET /organizations/{organization_id}/retention-policy
PUT /organizations/{organization_id}/retention-policy
GET /organizations/{organization_id}/oidc
PUT /organizations/{organization_id}/oidc
POST /organizations/{organization_id}/scim/token
POST /organizations/{organization_id}/bot-applications
GET /organizations/{organization_id}/bot-applications
GET /organizations/{organization_id}/bot-applications/{application_id}
POST /organizations/{organization_id}/bot-applications/{application_id}/tokens/rotate
POST /organizations/{organization_id}/bot-applications/{application_id}/spaces/{space_id}/invite
POST /organizations/{organization_id}/custom-domains
GET /organizations/{organization_id}/custom-domains
POST /organizations/{organization_id}/custom-domains/{custom_domain_id}/verify
GET /custom-domains/resolve
POST /scim/v2/Users
GET /scim/v2/Users/{external_id}
PATCH /scim/v2/Users/{external_id}
POST /organizations/{organization_id}/spaces
GET /organizations/{organization_id}/spaces
POST /organizations/{organization_id}/meetings
GET /organizations/{organization_id}/meetings
GET /meetings/{meeting_id}
GET /meetings/{meeting_id}/invite.ics
POST /meetings/{meeting_id}/calendar/caldav/sync
POST /meetings/{meeting_id}/calendar/google/sync
POST /meetings/{meeting_id}/calendar/microsoft/sync
PATCH /meetings/{meeting_id}
DELETE /meetings/{meeting_id}
POST /spaces/{space_id}/channels
GET /spaces/{space_id}/channels
PATCH /spaces/{space_id}
POST /spaces/{space_id}/members
DELETE /spaces/{space_id}/members/{user_id}
POST /spaces/{space_id}/roles
POST /spaces/{space_id}/roles/{role_id}/assignments
GET /spaces/{space_id}/audit-events
PATCH /channels/{channel_id}
DELETE /channels/{channel_id}
POST /channels/{channel_id}/permission-overrides
POST /channels/{channel_id}/webhooks
GET /channels/{channel_id}/webhooks
POST /channels/{channel_id}/webhooks/{webhook_id}/token/rotate
DELETE /channels/{channel_id}/webhooks/{webhook_id}
POST /channels/{channel_id}/command-interactions
POST /channels/{channel_id}/component-interactions
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

Discord compatibility message creation accepts `content`, basic `embeds`, and
`allowed_mentions`. Content may be empty when at least one embed or component is
supplied. Embed and component JSON is persisted and returned through
compatibility message responses and gateway `MESSAGE_CREATE` dispatches. Linked
native attachments are returned as Discord-shaped attachment metadata in
compatibility list responses and gateway dispatches. Compatibility message
create accepts same-channel `message_reference` replies and returns reply
metadata through REST and gateway; reply payloads also include a hydrated
Discord-shaped `referenced_message` when the referenced message is still
available. `allowed_mentions` now controls Discord-shaped user, role, and
everyone mention expansion in compatibility REST create/list/edit and gateway
payloads. Discord-compatible multipart message creates with `payload_json` and
`files[n]` store uploaded files as OpenCord attachments, link them to the new
message, and return them through REST and gateway payloads. Component clicks can
be created through the native `/channels/{channel_id}/component-interactions`
endpoint and dispatched to compatible gateway sessions as interaction type `3`.

## Enterprise OIDC

`PUT /organizations/{organization_id}/oidc` lets an organization owner/admin
configure an OIDC provider with issuer, endpoints, client credentials, allowed
email domains, SSO enforcement, and auto-join role. Responses never include the
stored client secret.

`GET /auth/oidc/providers?email=member@example.com` returns matching providers
for the email domain so clients can show the correct SSO option. When
`require_sso` is enabled for a matching domain, password registration/login is
blocked with `sso_required`.

`POST /auth/oidc/callback` accepts a validated provider assertion, creates or
reuses the local user, links the OIDC identity, creates a bearer session, and
auto-joins the configured organization. The current local assertion signature is
a testable development boundary; production OIDC code/JWKS exchange remains a
future hardening step.

## SCIM

`POST /organizations/{organization_id}/scim/token` rotates a SCIM bearer token
for an organization owner/admin. The token is shown once and should be stored in
the external identity provider.

`POST /scim/v2/Users` accepts a SCIM-like user payload with `externalId`,
`userName`, optional `name.formatted`, and `active`. It creates or reuses a
local user, links the external ID, and adds the user to the token's organization.

`GET /scim/v2/Users/{external_id}` returns the provisioned user for the SCIM
token's organization. `PATCH /scim/v2/Users/{external_id}` supports a SCIM
PatchOp that replaces `active` with a boolean and updates organization
membership status.

## Audit Export

`GET /organizations/{organization_id}/audit-events/export?from=<rfc3339>&to=<rfc3339>`
exports organization audit events in JSON for organization owners/admins. The
initial export path is synchronous and date-range scoped; asynchronous export
jobs with signed downloads remain future work.

## Data Export

`GET /organizations/{organization_id}/data-export?from=<rfc3339>&to=<rfc3339>`
exports organization messages and a linked file manifest in JSON for
organization owners/admins. The first implementation exports metadata and
attachment download URLs synchronously; asynchronous export packaging, signed
download archives, and export job audit events remain future work.

## Retention

`PUT /organizations/{organization_id}/retention-policy` lets an organization
owner/admin configure message, file, audit-log, and deleted-message retention
windows in days. `GET /organizations/{organization_id}/retention-policy` returns
the stored policy.

`opencord-worker` evaluates retention policies on a timer, records each
retention run, and purges expired messages, files, and audit events when dry-run
mode is disabled. Dry-run mode is enabled by default so operators can verify
counts before destructive purges.

## Cloud Tenants

`POST /cloud/tenants` provisions an organization and owner membership in one
store transaction while setting the initial `plan`, `deployment_mode`, and
`primary_region`. Normal self-hosted organization creation remains available at
`POST /organizations` and defaults to the `free` plan in `self_hosted` mode.

`GET /organizations/{organization_id}/usage` exposes billing/admin usage
counters for visible organizations, including active users, stored attachment
bytes, and connected calendar accounts for active members.

`POST /organizations/{organization_id}/custom-domains` creates a pending custom
domain mapping for the organization and returns a verification token. After
`POST /organizations/{organization_id}/custom-domains/{custom_domain_id}/verify`
activates the mapping, `GET /custom-domains/resolve` maps the request `Host`
header to tenant metadata for ingress and cloud routing.

## Billing

`POST /billing/provider-events` accepts normalized billing provider events,
stores local subscription state, and updates the organization's local plan
entitlement. Billing state is stored locally so request handling does not need
to query a billing provider for every request.

## Security Defaults

API and realtime responses include conservative browser security headers. CORS
is explicit: by default the server allows browser requests only from the
configured `OPENCORD_PUBLIC_URL` origin. Add comma-separated origins through
`OPENCORD_ALLOWED_ORIGINS` when a hosted web client or local Vite dev server
must connect cross-origin.

Local Compose sets:

```text
OPENCORD_ALLOWED_ORIGINS=http://localhost:5173
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
OPENCORD_RETENTION_POLL_SECONDS=3600
OPENCORD_RETENTION_DRY_RUN=true
```

The current dispatcher logs in-app, push, and email reminder deliveries. SMTP,
mobile push, and realtime in-app delivery adapters are future integration
points behind the same worker boundary.

## Calendar Sync

`POST /calendar/accounts/google`, `POST /calendar/accounts/microsoft`, and
`POST /calendar/accounts/caldav` connect the current user to a provider
calendar account for meeting sync. Access tokens, refresh tokens, and CalDAV
passwords are write-only request fields; responses expose only account metadata
and the token suffix.

`POST /meetings/{meeting_id}/calendar/{provider}/sync` creates or updates the
current user's provider event mapping for a meeting the user can manage. The
first implementation uses local Google, Microsoft, and CalDAV adapter
boundaries with durable provider event metadata, so internal meeting creation
does not depend on provider network availability.

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

## Bots

`POST /organizations/{organization_id}/bot-applications` lets an organization
admin create a bot application, internal bot user, and shown-once bot token.
Bot tokens are stored hashed and are issued with the `ocb_` prefix for later
compatibility routes under `/api/compat/discord/v10`.

`GET /organizations/{organization_id}/bot-applications` and
`GET /organizations/{organization_id}/bot-applications/{application_id}` return
organization-admin bot application details with active token last-four metadata
and bot-user space memberships. Raw bot tokens are only returned during create
or rotate operations.

`POST /organizations/{organization_id}/bot-applications/{application_id}/tokens/rotate`
lets an organization admin rotate a bot token. Existing tokens for that bot
application are deactivated, and the replacement `ocb_` token is shown once.

`POST /organizations/{organization_id}/bot-applications/{application_id}/spaces/{space_id}/invite`
adds the bot user to a space through the normal membership model. Bot
permissions and channel visibility continue to use the same OpenCord permission
checks as human users.

`POST /channels/{channel_id}/webhooks` lets a channel manager create an incoming
webhook for a text channel. The server creates an internal webhook bot user and
returns a shown-once `ocw_` token plus an execute URL. `POST
/api/webhooks/{webhook_id}/{webhook_token}` accepts a Discord-style webhook
payload with `content` and posts the message as the webhook bot user.
`GET /channels/{channel_id}/webhooks` lists active channel webhooks without raw
token material. `POST
/channels/{channel_id}/webhooks/{webhook_id}/token/rotate` returns a replacement
shown-once token and invalidates the old one. `DELETE
/channels/{channel_id}/webhooks/{webhook_id}` soft-disables the webhook so
existing execution URLs stop working.
Webhook create, token rotation, and delete write `webhook.created`,
`webhook.token_rotated`, and `webhook.deleted` audit events. Event metadata
includes webhook IDs and token last-four values, never raw `ocw_` token
material.
Public webhook execution is limited to 5 requests per minute per webhook URL
bucket. Successful execution responses include `X-RateLimit-Limit`,
`X-RateLimit-Remaining`, `X-RateLimit-Reset`, and `X-RateLimit-Bucket`.
Exhausted buckets return `429` with the same headers plus `Retry-After`.

`GET /api/compat/discord/v10/users/@me`,
`GET /api/compat/discord/v10/guilds/{space_id}`, and
`GET /api/compat/discord/v10/guilds/{space_id}/channels` support basic bot SDK
discovery flows. `GET /api/compat/discord/v10/guilds/{space_id}/roles`
returns custom space roles in a Discord-shaped role payload. Discovery routes
enforce normal OpenCord bot-user space membership and channel visibility
permissions where channels are involved.

`/api/compat/discord/v10/channels/{channel_id}/messages` supports the first
Discord-compatible bot message routes: send, list, edit, and delete. Requests
use `Authorization: Bot ocb_...`. The bot user must be added to the target
space through the normal member endpoint, and channel permissions are enforced
against that bot user.

Bot-authenticated compatibility REST routes are limited to 10 requests per
minute per bot application bucket. Successful responses include
`X-RateLimit-Limit`, `X-RateLimit-Remaining`, `X-RateLimit-Reset`, and
`X-RateLimit-Bucket`. Exhausted buckets return `429` with the same headers,
`Retry-After`, and Discord-shaped body `{ "message": "rate limit exceeded",
"code": 42900 }`.

`GET /api/compat/discord/gateway` upgrades to a Discord-shaped WebSocket. The
initial implementation sends HELLO, accepts IDENTIFY with an OpenCord bot
token, emits READY with bot-visible spaces as guilds, supports process-local
RESUME by `session_id`, acknowledges heartbeats, and dispatches CHANNEL_CREATE,
CHANNEL_UPDATE, CHANNEL_DELETE, GUILD_CREATE, GUILD_UPDATE, GUILD_MEMBER_ADD,
GUILD_MEMBER_REMOVE, MESSAGE_CREATE, MESSAGE_UPDATE, and MESSAGE_DELETE for
resources visible to the bot user. Message create, update, and delete
dispatches require the Discord `GUILD_MESSAGES` intent. Channel
create/update/delete and guild create/update dispatches require `GUILDS`;
guild-member add/remove dispatches require `GUILD_MEMBERS`. Unknown opcodes
close with `4001`; malformed payloads close with `4002`; invalid IDENTIFY
tokens close with `4004`; duplicate IDENTIFY frames close with `4005`; unknown
RESUME sessions close with `4009` after OP 9.

`POST /api/compat/discord/v10/applications/{application_id}/guilds/{space_id}/commands`
registers a space-scoped chat input command for the current bot application.
`POST /channels/{channel_id}/command-interactions` creates a local interaction
for a visible command and dispatches `INTERACTION_CREATE` to compatible gateway
sessions. `POST /channels/{channel_id}/component-interactions` creates a local
message component interaction for a clicked persisted `custom_id` and dispatches
Discord-shaped `INTERACTION_CREATE` type `3`. Bots respond through
`POST /api/compat/discord/v10/interactions/{interaction_id}/{interaction_token}/callback`
with callback type `4` to post a bot-authored channel message, or callback type
`5` to defer the response and then post one follow-up message through
`POST /api/compat/discord/v10/webhooks/{application_id}/{interaction_token}`.
Original interaction responses can be edited through
`PATCH /api/compat/discord/v10/webhooks/{application_id}/{interaction_token}/messages/@original`
and deleted through
`DELETE /api/compat/discord/v10/webhooks/{application_id}/{interaction_token}/messages/@original`.

## License

Elastic License 2.0 (`Elastic-2.0`).

## Contributing

Pull requests must follow [CONTRIBUTING.md](CONTRIBUTING.md). Human
contributors must sign the [Contributor License Agreement](CLA.md); the CLA
Assistant workflow checks PR comments and stores signatures in
`signatures/v1/cla.json`.
