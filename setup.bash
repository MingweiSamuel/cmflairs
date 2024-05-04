#!/bin/bash
set -euxo pipefail

which wrangler || npm i -g wrangler

QUEUE='dev-webjob'
$(wrangler queues list | grep -q "$QUEUE") || wrangler queues create "$QUEUE"

D1DB='dev-db'
$(wrangler d1 list | grep -q "$D1DB") || wrangler d1 create "$D1DB"
echo '❗ UPDATE YOUR `wrangler.toml` WITH THE ABOVE ❗'
