// Build the company universe from a statements file.
//
// The file is one row per company per period. Everything downstream works on the *latest*
// period, with the previous one attached as `prior*` so the two-period measures (Piotroski,
// growth) have something to compare against.

import { readDataset } from './pipeline.js';
import { analyse, normaliseStatement } from './fundamentals.js';

const PRIOR_FIELDS = [
  'revenue', 'netIncome', 'totalAssets', 'currentAssets', 'currentLiabilities',
  'longTermDebt', 'sharesOutstanding', 'grossProfit', 'costOfGoods',
];

/**
 * Load every company from a statements CSV.
 *
 * Returns companies with:
 *   symbol, name, sector, period, priorPeriod,
 *   statement   — the normalised latest period
 *   analysis    — the full fundamentals report
 *   metrics     — the flat map the screener and peer scoring read
 */
export async function loadUniverse(path) {
  const dataset = await readDataset(path);
  const rows = dataset.records || [];

  const bySymbol = new Map();
  for (const row of rows) {
    const symbol = row.company ?? row.symbol;
    if (!symbol) continue;
    if (!bySymbol.has(symbol)) bySymbol.set(symbol, []);
    bySymbol.get(symbol).push(row);
  }

  const companies = [];
  for (const [symbol, periods] of bySymbol) {
    periods.sort((a, b) => Number(b.period ?? 0) - Number(a.period ?? 0));
    const latestRow = periods[0];
    const priorRow = periods[1] || null;

    const statement = normaliseStatement(latestRow);
    if (priorRow) {
      const prior = normaliseStatement(priorRow);
      for (const field of PRIOR_FIELDS) {
        if (prior[field] !== undefined) {
          statement['prior' + field.charAt(0).toUpperCase() + field.slice(1)] = prior[field];
        }
      }
    }

    const analysis = analyse(statement);
    companies.push({
      symbol,
      name: latestRow.name ?? symbol,
      sector: latestRow.sector ?? 'unclassified',
      period: latestRow.period ?? null,
      priorPeriod: priorRow ? priorRow.period ?? null : null,
      statement,
      analysis,
      metrics: flattenMetrics(analysis),
    });
  }

  return companies.sort((a, b) => a.symbol.localeCompare(b.symbol));
}

/**
 * Flatten the nested analysis into the metric names a screen refers to.
 *
 * Only numeric values survive: a screen on a null metric must exclude the company, and a
 * `null` in this map would read as "measured, and bad".
 */
export function flattenMetrics(analysis) {
  const out = {};
  for (const group of ['profitability', 'efficiency', 'liquidity', 'leverage', 'perShare', 'valuation']) {
    for (const [key, value] of Object.entries(analysis[group] || {})) {
      if (typeof value === 'number' && Number.isFinite(value)) out[key] = value;
    }
  }
  const composites = analysis.composites || {};
  if (composites.piotroskiF) out.piotroskiF = composites.piotroskiF.score;
  if (composites.altmanZ) out.altmanZ = composites.altmanZ.z;
  if (composites.dupont) out.dupontRoe = composites.dupont.roe;
  if (composites.ruleOf40) out.ruleOf40 = composites.ruleOf40.total;
  return out;
}

/** One company by symbol, or null. */
export function findBySymbol(companies, symbol) {
  const wanted = String(symbol || '').toUpperCase();
  return companies.find((c) => c.symbol.toUpperCase() === wanted) || null;
}
