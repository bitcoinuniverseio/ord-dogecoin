Security
========

Threat model in one line
------------------------

This service reads a public blockchain and serves it. It holds no keys, moves
no funds, and accepts no writes. The risks are **serving hostile bytes to
browsers**, **exposing an unauthenticated surface**, and **being trusted for
more than it can prove**.

The server has no authentication
--------------------------------

There is no auth, no API keys, no rate limiting, no per-caller quotas, and no
request budget. Additionally:

- `--address` defaults to **`0.0.0.0`**, every interface.
- `--http-port` defaults to 80 and `--https-port` to 443.
- `Access-Control-Allow-Origin: *` is set unconditionally, so any web page in
  any browser can read the instance.

Bind to `127.0.0.1` and put a reverse proxy in front that owns TLS,
authentication, rate limiting and request size limits. The reference systemd
unit uses `--address=127.0.0.1` with a high port.

Some routes are unbounded by design: `/api/v1/drc20/transferables` walks every
outstanding transferable, and the unpaginated address routes scale with the
address. An unauthenticated instance is trivially expensive to query.

Serving inscription content
---------------------------

`/content/:id` serves attacker-controlled bytes with an attacker-controlled
content type, and `/preview/:id` renders them. Inscription content on this
chain has **no size limit**, so it can also be very large.

The content route sets a Content-Security-Policy that permits
`'unsafe-eval'`, `'unsafe-inline'`, `data:` and `blob:`. That is not an
oversight; inscriptions are frequently self-contained HTML and SVG that need
it. The consequence is that inscription content executes script in the browser.

Set `--csp-origin` to the exact public-facing origin of the instance. Without
it, the fallback policy is written in terms of `'self'` and wildcard origins,
which is much weaker.

Then keep the blast radius small:

- **Serve content from a separate origin** from anything with a session cookie
  or an admin interface. Same-origin script from an inscription can reach
  whatever that origin can reach.
- Never serve content from the same origin as an authenticated application.
- Content responses carry `Cache-Control: max-age=31536000, immutable`, which
  is correct for immutable content and means a CDN in front is effective and
  cheap.

The `hidden` list in the YAML config removes specific inscription ids from
`/content` and `/preview` on **one instance**. It is a per-instance moderation
control, not deletion: the data stays in the index, and any other instance
serving the same database serves it normally.

RPC credentials
---------------

Two authentication paths, in this order:

1. If the cookie file exists, it is used.
2. Otherwise, the username and password are parsed out of `--rpc-url`.

**Prefer the cookie file.** A URL with embedded credentials appears in the
process list, in shell history, in systemd unit files, and in the log line the
process writes on startup.

Keep the cookie file readable only by the service user. The reference
deployment mounts it read-only and runs under a dedicated unprivileged account
with a supplementary group for node access.

Filesystem and process isolation
--------------------------------

The reference unit
`deploy/linux/universe-ord-dogecoin-full.service` applies the standard systemd
hardening set: `NoNewPrivileges=true`, `PrivateTmp=true`,
`ProtectSystem=strict`, `ProtectHome=true`, `ProtectKernelTunables=true`,
`ProtectKernelModules=true`, `ProtectKernelLogs=true`,
`ProtectControlGroups=true`, `RestrictNamespaces=true`,
`RestrictRealtime=true`, `RestrictSUIDSGID=true`,
`RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX`,
`SystemCallArchitectures=native`, `UMask=0027`, and an explicit
`ReadWritePaths=` allowlist. Copy it.

Note that `docker-compose.yaml` runs the container `privileged: true` with
`network_mode: host`. That is convenient for reaching a node on the host's
loopback address and it is not a hardened configuration. Do not run it that way
on a shared host.

What this index can and cannot prove
------------------------------------

It can prove: what a Dogecoin node told it about confirmed blocks, up to the
height in `block_count`.

It cannot prove:

- **Anything about unconfirmed transactions.** There is no mempool. An output
  spent by an unconfirmed transaction still looks unspent here.
- **That a UTXO is safe to spend.** `/api/v1/funding/:address` returns the raw
  previous transaction precisely so a caller can verify the prevout against a
  node rather than trusting this index. Its own contract says callers must
  apply their own reservations first.
- **That its answers survive a reorg.** Below about 40 blocks the index can
  roll back and reindex. Above that it fails, and its answers are wrong until
  it is rebuilt.
- **That another instance agrees with it.** Compare `subsidy_schedule_hash`.

The Universe capability snapshot records, for both `doginals` and `drc20`, that
settlement "requires the exact Dogecoin transaction confirmation and the
expected protocol-state transition in a later authoritative checkpoint", and
that execution requires "distinct read and execution credentials, a fresh
execution-authenticated readiness probe, a complete authoritative checkpoint,
Core-verified funding, exact immutable economics, and protocol-native
post-sign validation". Those gates live in the consuming authority. Nothing in
this repository enforces them.

Dependency provenance
---------------------

`Cargo.toml` patches `bitcoin` and `bitcoincore-rpc` to
`github.com/apezord/rust-dogecoin` and
`github.com/apezord/rust-dogecoincore-rpc`. Those are third-party Git
dependencies, not crates.io releases, and they are consensus-relevant: they
carry Dogecoin's address version bytes and serialization.

Always build with `--locked` so `Cargo.lock` pins the exact commits.

Reporting a vulnerability
-------------------------

See [`SECURITY.md`](https://github.com/bitcoinuniverseio/ord-dogecoin/blob/develop/SECURITY.md)
at the repository root. Do not open a public issue for a vulnerability.
