// Screening, ranking and peer comparison for a stock-analysis application.
//
// Pure functions over an array of "companies", where a company is
//   { symbol, name, sector, ...statement fields..., metrics?: { roe: 0.45, ... } }
//
// The rule that keeps this honest: a company missing a metric is *excluded* from a screen on
// that metric rather than treated as zero. Treating "unknown" as "bad" quietly buries companies
// whose data has not been loaded yet, which is the most common way a screener lies.

const isNum = (v) => typeof v === 'number' && Number.isFinite(v);

/** Percentile rank of a value within a list, 0..1. Null when the list has no numbers. */
export function percentileRank(value, values) {
  const numbers = values.filter(isNum);
  if (!isNum(value) || numbers.length === 0) return null;
  const below = numbers.filter((v) => v < value).length;
  const equal = numbers.filter((v) => v === value).length;
  // Mid-rank for ties, so identical companies get identical percentiles.
  return (below + equal / 2) / numbers.length;
}

const TESTS = {
  gt: (a, b) => a > b,
  gte: (a, b) => a >= b,
  lt: (a, b) => a < b,
  lte: (a, b) => a <= b,
  eq: (a, b) => a === b,
};

/**
 * Filter companies by a list of criteria.
 *
 * criteria: [{ metric, op, value }] where op is one of gt, gte, lt, lte, eq.
 * A company that lacks the metric for any criterion does not pass — and is counted as
 * `excluded`, not as `failed`, so the caller can tell the two apart.
 */
export function screen(companies, criteria) {
  const rules = Array.isArray(criteria) ? criteria : [];
  const passed = [];
  const failed = [];
  const excluded = [];

  for (const company of companies) {
    let verdict = 'pass';
    for (const rule of rules) {
      const test = TESTS[rule.op];
      if (!test) throw new Error(`unknown operator ${rule.op}; use ${Object.keys(TESTS).join(', ')}`);
      const value = metricOf(company, rule.metric);
      if (!isNum(value)) {
        verdict = 'excluded';
        break;
      }
      if (!test(value, rule.value)) {
        verdict = 'fail';
        break;
      }
    }
    if (verdict === 'pass') passed.push(company);
    else if (verdict === 'fail') failed.push(company);
    else excluded.push(company);
  }

  return { passed, failed, excluded, total: companies.length };
}

/** Read a metric from `company.metrics`, falling back to a top-level field. */
export function metricOf(company, metric) {
  if (!company || !metric) return null;
  if (company.metrics && isNum(company.metrics[metric])) return company.metrics[metric];
  if (isNum(company[metric])) return company[metric];
  return null;
}

/** Sort by a metric, best first. Companies missing it go last, in stable order. */
export function rank(companies, metric, direction = 'desc') {
  const sign = direction === 'desc' ? -1 : 1;
  return companies.slice().sort((a, b) => {
    const left = metricOf(a, metric);
    const right = metricOf(b, metric);
    if (!isNum(left) && !isNum(right)) return 0;
    if (!isNum(left)) return 1;
    if (!isNum(right)) return -1;
    return (left - right) * sign;
  });
}

/** Add a percentile for one metric to every company that has it. */
export function withPercentile(companies, metric) {
  const values = companies.map((c) => metricOf(c, metric)).filter(isNum);
  return companies.map((company) => {
    const value = metricOf(company, metric);
    return {
      ...company,
      percentile: isNum(value) ? percentileRank(value, values) : null,
    };
  });
}

/** Median of a list, or null. Used instead of a mean so one outlier cannot move a sector. */
export function median(values) {
  const numbers = values.filter(isNum).slice().sort((a, b) => a - b);
  if (numbers.length === 0) return null;
  const mid = Math.floor(numbers.length / 2);
  return numbers.length % 2 ? numbers[mid] : (numbers[mid - 1] + numbers[mid]) / 2;
}

/** Per-sector statistics for one metric, with each company's position inside its sector. */
export function sectorSummary(companies, metric) {
  const bySector = new Map();
  for (const company of companies) {
    const sector = company.sector || 'unclassified';
    if (!bySector.has(sector)) bySector.set(sector, []);
    bySector.get(sector).push(company);
  }

  const out = [];
  for (const [sector, members] of bySector) {
    const values = members.map((c) => metricOf(c, metric)).filter(isNum);
    out.push({
      sector,
      count: members.length,
      measured: values.length,
      median: median(values),
      best: values.length ? Math.max(...values) : null,
      worst: values.length ? Math.min(...values) : null,
      members: members.map((c) => ({
        symbol: c.symbol,
        name: c.name,
        value: metricOf(c, metric),
        percentile: isNum(metricOf(c, metric)) ? percentileRank(metricOf(c, metric), values) : null,
      })),
    });
  }
  return out.sort((a, b) => b.measured - a.measured || a.sector.localeCompare(b.sector));
}

/**
 * Peer-relative score.
 *
 * A raw ROE of 20% means different things in software and in steel, so each metric is scored
 * against the company's own sector. Metrics where lower is better are inverted.
 */
const LOWER_IS_BETTER = new Set(['peRatio', 'pbRatio', 'psRatio', 'evEbitda', 'debtToEquity', 'debtToAssets']);

export function peerScore(companies, metrics = ['roe', 'roic', 'netMargin', 'debtToEquity']) {
  const sectors = sectorSummary(companies, metrics[0]);
  const sectorOf = new Map();
  for (const entry of sectors) {
    for (const member of entry.members) sectorOf.set(member.symbol, entry);
  }

  return companies.map((company) => {
    const sectorEntry = sectorOf.get(company.symbol);
    let total = 0;
    let counted = 0;
    const parts = [];

    for (const metric of metrics) {
      const value = metricOf(company, metric);
      if (!isNum(value) || !sectorEntry) continue;
      const values = companies
        .filter((c) => (c.sector || 'unclassified') === sectorEntry.sector)
        .map((c) => metricOf(c, metric))
        .filter(isNum);
      let percentile = percentileRank(value, values);
      if (percentile === null) continue;
      if (LOWER_IS_BETTER.has(metric)) percentile = 1 - percentile;
      parts.push({ metric, value, percentile });
      total += percentile;
      counted += 1;
    }

    return {
      symbol: company.symbol,
      name: company.name,
      sector: company.sector || 'unclassified',
      score: counted ? total / counted : null,
      metricsUsed: counted,
      parts,
    };
  });
}

/** Rank companies by peer score, best first. */
export function rankByPeerScore(companies, metrics) {
  return peerScore(companies, metrics)
    .slice()
    .sort((a, b) => {
      if (a.score === null && b.score === null) return 0;
      if (a.score === null) return 1;
      if (b.score === null) return -1;
      return b.score - a.score;
    });
}
