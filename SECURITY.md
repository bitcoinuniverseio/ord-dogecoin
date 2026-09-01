# Security policy

## Reporting a vulnerability

Report privately, before anything public. Open a private report at
[Report a vulnerability](https://github.com/bitcoinuniverseio/ord-dogecoin/security/advisories/new).
Only the maintainers can read it, and you keep the thread with them until it is
resolved.

Do not open a public issue, a pull request, or a public post for a suspected
vulnerability.

Include:

- what the issue is, and the smallest steps that reproduce it;
- the commit or build affected;
- which index feature flags the affected database was created with, and the
  output of `GET /api/v1/capabilities`;
- what an attacker could achieve.

Please allow time for a fix before publishing.

## Supported versions

This repository publishes no releases. The tags `1.0`, `1.0.1` and `1.0.2` are
upstream's, inherited with the fork history, and are not maintained here.

Fixes land on `develop` and are promoted to `main`. Those two branches are what
is supported.

## In scope

- The indexer and HTTP server in this repository.
- The `/api/v1` contract and `openapi.yaml`.
- Index correctness: a way to make the index record ownership, balances, or
  protocol state that does not follow from the chain.
- The deployment units and scripts under `deploy/`.
- Documentation that describes a safety property the code does not have.

Particularly interesting:

- A crafted transaction or inscription that panics the indexer, corrupts the
  database, or makes the index disagree with the chain.
- An input that escapes the sandboxing of `/content` or `/preview` in a way the
  Content-Security-Policy is supposed to prevent.
- An `/api/v1` response that misrepresents index completeness or capability, so
  a consumer treats a partial answer as a complete one.

## Out of scope

- **The absence of authentication and rate limiting on `ord server`.** This is
  documented and intended: the server is meant to sit behind a reverse proxy
  that owns TLS, auth and rate limiting. See
  [Security](docs/src/dogecoin/security.md). A report that an exposed instance
  can be queried by anyone is not a vulnerability in this software.
- **Inscription content executing script in a browser.** Content responses
  deliberately permit `'unsafe-eval'`, `'unsafe-inline'`, `data:` and `blob:`,
  because inscriptions are frequently self-contained HTML. Serve content from
  its own origin. A report that becomes interesting is one where content
  escapes the origin it was served from.
- **The `ord wallet` subcommands.** They are inherited upstream code that
  cannot function against Dogecoin Core, and they are not part of any
  deployment.
- Vulnerabilities in Dogecoin Core itself. Report those to the Dogecoin
  project.
- Denial of service through expensive but documented queries, for an instance
  that is exposed without a proxy.

## What we ask you not to do

- Do not run load tests, automated scanners, or denial of service attempts
  against a production instance.
- Do not use a real transaction to demonstrate a defect when a description or a
  regtest reproduction shows the same thing.

## What nobody will ever ask you for

A private key, a seed phrase, or a signed transaction that has not been
broadcast. None of those are needed to reproduce a defect in an indexer, and
they cannot be un-sent. Anyone who asks is committing fraud, regardless of who
they appear to be.
