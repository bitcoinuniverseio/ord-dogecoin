#!/bin/bash
# Doginals authority (ord-accel-v9). Since 2026-09-16 the Dogecoin RPC is the
# shared Universe Dogecoin Core on universe-indexers-1, reached through the
# loopback tunnel universe-dogecoin-rpc-tunnel.service (127.0.0.1:22566).
# No local Dogecoin Core runs on this host any more (its chain data was lost
# to disk exhaustion on 2026-09-15); do not re-enable universe-dogecoin.service.
export SUBSIDIES_PATH=/usr/local/bin/subsidies.json
export STARTING_SATS_PATH=/usr/local/bin/starting_sats.json
export RUST_LOG=info
ulimit -n 1048576

exec /usr/local/bin/ord-accel-v9   --rpc-url=http://doge:<rpc-password>@127.0.0.1:22566   --index=/var/lib/universe-ord-0.29/ord-dogecoin/doginals.redb   --first-inscription-height=4609723   --db-cache-size=4000000000   --nr-parallel-requests=8   server --http --address=127.0.0.1 --http-port=8390
