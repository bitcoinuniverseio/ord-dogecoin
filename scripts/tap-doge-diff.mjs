#!/usr/bin/env node
// Compare the Universe TAP-DOGE engine (/r/tap) with Trac's TAP-DOGE reader
// channel for the keys Trac's peers still serve. Used to calibrate Dogecoin
// activation heights (docs/TAP-DOGE-ENGINE-PLAN.md).
//
//   node tap-doge-diff.mjs [--ticks a,b] [--rows 200]
//
// Environment:
//   TRAC_READER   Trac reader base URL (default https://127.0.0.1:38317/reader)
//   TRAC_CA       PEM CA for the Trac reader gateway (optional)
//   TRAC_SNI      TLS server name for the gateway (optional)
//   ENGINE        engine base URL (default http://127.0.0.1:38393/r/tap)
//
// Only rows at or below the engine's indexed height are compared, so the
// engine can be checked while it is still replaying.
import https from 'node:https';
import http from 'node:http';
import fs from 'node:fs';

const args = process.argv.slice(2);
const opt = (name, fallback) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 ? args[i + 1] : fallback;
};
const TRAC = process.env.TRAC_READER || 'https://127.0.0.1:38317/reader';
const ENGINE = process.env.ENGINE || 'http://127.0.0.1:38393/r/tap';
const ca = process.env.TRAC_CA ? fs.readFileSync(process.env.TRAC_CA) : undefined;
const servername = process.env.TRAC_SNI || undefined;
const ROWS = Number(opt('rows', '200'));

function get(base, path, timeoutMs = 30000) {
  const url = new URL(base + path);
  const lib = url.protocol === 'https:' ? https : http;
  return new Promise((resolve) => {
    const req = lib.get(
      url,
      { ca, servername, timeout: timeoutMs, headers: { accept: 'application/json' } },
      (res) => {
        let body = '';
        res.on('data', (d) => (body += d));
        res.on('end', () => {
          try {
            resolve({ ok: res.statusCode === 200, value: JSON.parse(body).result });
          } catch {
            resolve({ ok: false, value: null });
          }
        });
      },
    );
    req.on('timeout', () => req.destroy());
    req.on('error', () => resolve({ ok: false, value: null }));
  });
}

const DEPLOY_FIELDS = ['tick', 'max', 'lim', 'dec', 'blck', 'tx', 'vo', 'ins', 'num', 'addr', 'dmt', 'elem', 'prj', 'dim', 'dt'];
const ROW_FIELDS = ['addr', 'blck', 'amt', 'bal', 'tx', 'vo', 'ins', 'num', 'fail', 'dmtblck', 'rcv', 'tick'];

function diffFields(a, b, fields) {
  return fields.filter((f) => JSON.stringify(a?.[f] ?? null) !== JSON.stringify(b?.[f] ?? null));
}

async function list(base, path, length) {
  const rows = [];
  for (let offset = 0; offset < length; offset += 100) {
    const r = await get(base, `${path}?offset=${offset}&max=${Math.min(100, length - offset)}`);
    if (!r.ok || !Array.isArray(r.value)) return null;
    rows.push(...r.value);
  }
  return rows;
}

const height = (await get(ENGINE, '/getCurrentBlock')).value;
console.log(`engine height ${height}`);

let ticks = opt('ticks', '')
  .split(',')
  .filter(Boolean);
if (!ticks.length) {
  const n = (await get(TRAC, '/getDeploymentsLength')).value || 0;
  const deps = await list(TRAC, '/getDeployments', n);
  ticks = (deps || []).filter((d) => d.blck <= height).map((d) => d.tick);
}
console.log(`comparing ${ticks.length} deployments at or below ${height}`);

const summary = { deployOk: 0, deployDiff: 0, deployMissing: 0, mintRowsOk: 0, mintRowsDiff: 0, sentRowsOk: 0, sentRowsDiff: 0 };
for (const tick of ticks) {
  const enc = encodeURIComponent(tick);
  const [t, e] = await Promise.all([get(TRAC, `/getDeployment/${enc}`), get(ENGINE, `/getDeployment/${enc}`)]);
  if (!e.value) {
    summary.deployMissing++;
    console.log(`MISSING deploy ${tick} (trac blck ${t.value?.blck})`);
    continue;
  }
  const df = diffFields(t.value, e.value, DEPLOY_FIELDS);
  if (df.length) {
    summary.deployDiff++;
    console.log(`DIFF deploy ${tick}: ${df.map((f) => `${f} trac=${JSON.stringify(t.value?.[f])} ours=${JSON.stringify(e.value?.[f])}`).join('; ')}`);
  } else summary.deployOk++;

  for (const [name, path, okKey, diffKey] of [
    ['mint', 'getTickerMintList', 'mintRowsOk', 'mintRowsDiff'],
    ['sent', 'getTickerSentList', 'sentRowsOk', 'sentRowsDiff'],
  ]) {
    const [tl, el] = await Promise.all([get(TRAC, `/${path}Length/${enc}`), get(ENGINE, `/${path}Length/${enc}`)]);
    if (!tl.ok || !el.ok) continue;
    const n = Math.min(ROWS, tl.value || 0, el.value || 0);
    if (!n) continue;
    const [tr, er] = await Promise.all([list(TRAC, `/${path}/${enc}`, n), list(ENGINE, `/${path}/${enc}`, n)]);
    if (!tr || !er) continue;
    for (let i = 0; i < n; i++) {
      if (tr[i]?.blck > height) break;
      const d = diffFields(tr[i], er[i], ROW_FIELDS);
      if (d.length) {
        summary[diffKey]++;
        if (summary[diffKey] <= 20)
          console.log(`DIFF ${name} ${tick}#${i} blck ${tr[i]?.blck}: ${d.map((f) => `${f} trac=${JSON.stringify(tr[i]?.[f])} ours=${JSON.stringify(er[i]?.[f])}`).join('; ')}`);
      } else summary[okKey]++;
    }
  }
}
console.log(JSON.stringify(summary));
