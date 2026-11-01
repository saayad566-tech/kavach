import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { loadUniverse, findBySymbol } from './statements.js';
import { screen, rank, sectorSummary, rankByPeerScore } from './screener.js';
import { dcf, sensitivity, marginOfSafety } from './dcf.js';

const here = path.dirname(fileURLToPath(import.meta.url));
const publicDir = path.join(here, '..', 'public');
const statementsFile = process.env.STOCKLAB_DATA || path.join(here, '..', 'data', 'statements.csv');

const TYPES = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
};

function json(res, status, body) {
  const payload = JSON.stringify(body);
  res.writeHead(status, {
    'content-type': 'application/json; charset=utf-8',
    'content-length': Buffer.byteLength(payload),
  });
  res.end(payload);
}

// One read per process: the statements file does not change while the server runs.
let universe = null;
async function companies() {
  if (!universe) universe = await loadUniverse(statementsFile);
  return universe;
}

/** Parse `?metric=roe&op=gt&value=0.2` (repeatable) into screener criteria. */
function criteriaFrom(params) {
  const metrics = params.getAll('metric');
  const ops = params.getAll('op');
  const values = params.getAll('value');
  return metrics.map((metric, index) => ({
    metric,
    op: ops[index] || 'gt',
    value: Number(values[index]),
  })).filter((rule) => rule.metric && Number.isFinite(rule.value));
}

export function createApp() {
  return createServer(async (req, res) => {
    const url = new URL(req.url || '/', 'http://localhost');
    try {
      // ---- API -----------------------------------------------------------
      if (url.pathname === '/api/health') {
        return json(res, 200, { ok: true, companies: (await companies()).length });
      }

      if (url.pathname === '/api/companies') {
        const list = await companies();
        return json(res, 200, {
          companies: list.map((c) => ({
            symbol: c.symbol,
            name: c.name,
            sector: c.sector,
            period: c.period,
            priorPeriod: c.priorPeriod,
            metrics: c.metrics,
            flags: c.analysis.flags,
          })),
        });
      }

      if (url.pathname.startsWith('/api/company/')) {
        const symbol = decodeURIComponent(url.pathname.split('/').pop());
        const company = findBySymbol(await companies(), symbol);
        if (!company) return json(res, 404, { error: `no company ${symbol}` });
        return json(res, 200, company);
      }

      if (url.pathname === '/api/screen') {
        const list = await companies();
        const result = screen(list, criteriaFrom(url.searchParams));
        return json(res, 200, {
          passed: result.passed.map((c) => c.symbol),
          failed: result.failed.map((c) => c.symbol),
          excluded: result.excluded.map((c) => c.symbol),
          total: result.total,
        });
      }

      if (url.pathname === '/api/rank') {
        const metric = url.searchParams.get('metric') || 'roe';
        const direction = url.searchParams.get('dir') || 'desc';
        const ranked = rank(await companies(), metric, direction);
        return json(res, 200, {
          metric,
          direction,
          rows: ranked.map((c) => ({
            symbol: c.symbol,
            name: c.name,
            sector: c.sector,
            value: c.metrics[metric] ?? null,
          })),
        });
      }

      if (url.pathname === '/api/sectors') {
        const metric = url.searchParams.get('metric') || 'roe';
        return json(res, 200, { metric, sectors: sectorSummary(await companies(), metric) });
      }

      if (url.pathname === '/api/peers') {
        const metrics = (url.searchParams.get('metrics') || 'roe,roic,netMargin,debtToEquity')
          .split(',').map((m) => m.trim()).filter(Boolean);
        return json(res, 200, { metrics, ranking: rankByPeerScore(await companies(), metrics) });
      }

      if (url.pathname.startsWith('/api/dcf/')) {
        const symbol = decodeURIComponent(url.pathname.split('/').pop());
        const company = findBySymbol(await companies(), symbol);
        if (!company) return json(res, 404, { error: `no company ${symbol}` });

        const q = (name, fallback) => {
          const raw = url.searchParams.get(name);
          const value = raw === null ? NaN : Number(raw);
          return Number.isFinite(value) ? value : fallback;
        };
        const fcf = company.metrics.freeCashFlow ?? company.statement.freeCashFlow;
        if (!Number.isFinite(fcf)) {
          return json(res, 400, { error: `${symbol} has no free cash flow in its statements` });
        }

        const inputs = {
          freeCashFlow: fcf,
          growth: q('growth', 0.05),
          discount: q('discount', 0.10),
          terminalGrowth: q('terminal', 0.025),
          years: q('years', 10),
          shares: company.statement.sharesOutstanding,
          netDebt: company.metrics.netDebt ?? 0,
        };
        const result = dcf(inputs);
        if (!result || result.error) {
          return json(res, 400, { error: result ? result.error : 'the inputs cannot produce a value', inputs });
        }
        const price = company.statement.price ?? null;
        return json(res, 200, {
          symbol,
          inputs,
          result,
          price,
          marginOfSafety: marginOfSafety(result.perShare, price),
          sensitivity: sensitivity(inputs),
        });
      }

      // ---- static --------------------------------------------------------
      const relative = url.pathname === '/' ? 'index.html' : url.pathname.replace(/^\/+/, '');
      try {
        const body = await readFile(path.join(publicDir, relative));
        res.writeHead(200, { 'content-type': TYPES[path.extname(relative)] || 'application/octet-stream' });
        return res.end(body);
      } catch {
        return json(res, 404, { error: 'not found' });
      }
    } catch (err) {
      return json(res, 500, { error: err.message });
    }
  });
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const port = Number(process.env.PORT || 4270);
  createApp().listen(port, '127.0.0.1', () => {
    console.log(`StockLab on http://127.0.0.1:${port}`);
  });
}
