// Analysis functions for a generated Kavach application.
//
// Every function here is pure: it takes records and returns a value, never touching the
// filesystem or the network. That is what makes them testable and what keeps the app
// honest about doing its work on this machine.
//
// A "record" is a plain object, e.g. { date: '2026-01-01', category: 'alpha', amount: 101.5 }.
// Fields are addressed by name. Anything that cannot be computed returns null rather than
// NaN, so a caller can tell "no data" from "the answer is zero".

const isNum = (v) => typeof v === 'number' && Number.isFinite(v);
const numbers = (records, field) => records.map((r) => r[field]).filter(isNum);
const byField = (a, b) => (a < b ? -1 : a > b ? 1 : 0);

export function count(records) {
  return Array.isArray(records) ? records.length : 0;
}

export function sumField(records, field) {
  const values = numbers(records, field);
  return values.length ? values.reduce((total, v) => total + v, 0) : null;
}

export function meanField(records, field) {
  const total = sumField(records, field);
  const n = numbers(records, field).length;
  return total === null || n === 0 ? null : total / n;
}

export function medianField(records, field) {
  const values = numbers(records, field).slice().sort((a, b) => a - b);
  if (!values.length) return null;
  const mid = Math.floor(values.length / 2);
  return values.length % 2 ? values[mid] : (values[mid - 1] + values[mid]) / 2;
}

export function modeField(records, field) {
  const counts = frequency(records, field);
  return counts.length ? counts[0].value : null;
}

export function minField(records, field) {
  const values = numbers(records, field);
  return values.length ? Math.min(...values) : null;
}

export function maxField(records, field) {
  const values = numbers(records, field);
  return values.length ? Math.max(...values) : null;
}

export function varianceField(records, field) {
  const values = numbers(records, field);
  if (values.length < 2) return null;
  const mean = values.reduce((t, v) => t + v, 0) / values.length;
  // Sample variance: the records are a sample of the phenomenon, not the whole of it.
  return values.reduce((t, v) => t + (v - mean) ** 2, 0) / (values.length - 1);
}

export function stddevField(records, field) {
  const v = varianceField(records, field);
  return v === null ? null : Math.sqrt(v);
}

/** Nearest-rank percentile. p is 0..1. */
export function percentileField(records, field, p) {
  const values = numbers(records, field).slice().sort((a, b) => a - b);
  if (!values.length || !(p >= 0 && p <= 1)) return null;
  const rank = Math.ceil(p * values.length) - 1;
  return values[Math.min(Math.max(rank, 0), values.length - 1)];
}

/** Value counts, most frequent first, ties broken by value. */
export function frequency(records, field) {
  const counts = new Map();
  for (const record of records) {
    const value = record[field];
    if (value === undefined || value === null || value === '') continue;
    counts.set(value, (counts.get(value) || 0) + 1);
  }
  return [...counts.entries()]
    .map(([value, n]) => ({ value, count: n }))
    .sort((a, b) => b.count - a.count || byField(a.value, b.value));
}

export function distinctCount(records, field) {
  return frequency(records, field).length;
}

const AGGREGATORS = {
  sum: (values) => values.reduce((t, v) => t + v, 0),
  mean: (values) => values.reduce((t, v) => t + v, 0) / values.length,
  min: (values) => Math.min(...values),
  max: (values) => Math.max(...values),
  count: (values) => values.length,
};

/** Group by one field and aggregate another. Unknown aggregators throw rather than guess. */
export function groupAggregate(records, keyField, valueField, how = 'sum') {
  const aggregate = AGGREGATORS[how];
  if (!aggregate) throw new Error(`unknown aggregate ${how}; use ${Object.keys(AGGREGATORS).join(', ')}`);

  const groups = new Map();
  for (const record of records) {
    const key = record[keyField];
    if (key === undefined || key === null || key === '') continue;
    if (!groups.has(key)) groups.set(key, []);
    const value = record[valueField];
    if (how === 'count' || isNum(value)) groups.get(key).push(how === 'count' ? 1 : value);
  }

  return [...groups.entries()]
    .filter(([, values]) => values.length > 0)
    .map(([key, values]) => ({ key, value: aggregate(values), n: values.length }))
    .sort((a, b) => b.value - a.value);
}

export function topN(records, field, n = 5, direction = 'desc') {
  const sorted = records
    .filter((r) => isNum(r[field]))
    .slice()
    .sort((a, b) => (direction === 'asc' ? a[field] - b[field] : b[field] - a[field]));
  return sorted.slice(0, Math.max(0, n));
}

/** Sort by a field; numbers numerically, everything else as strings. Stable. */
export function sortBy(records, field, direction = 'asc') {
  const sign = direction === 'desc' ? -1 : 1;
  return records.slice().sort((a, b) => {
    const left = a[field];
    const right = b[field];
    if (isNum(left) && isNum(right)) return (left - right) * sign;
    return byField(String(left ?? ''), String(right ?? '')) * sign;
  });
}

/** Filter with a small, explicit spec rather than a callback string. */
export function filterWhere(records, spec) {
  const { field, op, value } = spec || {};
  const tests = {
    eq: (a) => a === value,
    ne: (a) => a !== value,
    gt: (a) => isNum(a) && a > value,
    gte: (a) => isNum(a) && a >= value,
    lt: (a) => isNum(a) && a < value,
    lte: (a) => isNum(a) && a <= value,
    contains: (a) => String(a ?? '').toLowerCase().includes(String(value ?? '').toLowerCase()),
    in: (a) => Array.isArray(value) && value.includes(a),
  };
  const test = tests[op];
  if (!test) throw new Error(`unknown operator ${op}; use ${Object.keys(tests).join(', ')}`);
  return records.filter((record) => test(record[field]));
}

export function paginate(records, page = 1, size = 25) {
  const safeSize = Math.max(1, Math.floor(size));
  const safePage = Math.max(1, Math.floor(page));
  const total = records.length;
  const start = (safePage - 1) * safeSize;
  return {
    items: records.slice(start, start + safeSize),
    page: safePage,
    size: safeSize,
    total,
    pages: Math.max(1, Math.ceil(total / safeSize)),
  };
}

/** Period-over-period change of a value field, ordered by a date field. */
export function growthRate(records, dateField, valueField) {
  const ordered = records
    .filter((r) => r[dateField] != null && isNum(r[valueField]))
    .slice()
    .sort((a, b) => byField(String(a[dateField]), String(b[dateField])));
  const out = [];
  for (let i = 1; i < ordered.length; i += 1) {
    const previous = ordered[i - 1][valueField];
    const current = ordered[i][valueField];
    const change = previous === 0 ? null : (current - previous) / previous;
    out.push({ from: ordered[i - 1][dateField], to: ordered[i][dateField], change });
  }
  return out;
}

/** Compound growth between two values over a number of periods. */
export function cagr(first, last, periods) {
  if (!isNum(first) || !isNum(last) || !isNum(periods) || periods <= 0 || first <= 0 || last <= 0) return null;
  return (last / first) ** (1 / periods) - 1;
}

export function movingAverage(values, window = 3) {
  const size = Math.max(1, Math.floor(window));
  if (!Array.isArray(values) || values.length < size) return [];
  const out = [];
  for (let i = size - 1; i < values.length; i += 1) {
    const slice = values.slice(i - size + 1, i + 1);
    if (!slice.every(isNum)) {
      out.push(null);
      continue;
    }
    out.push(slice.reduce((t, v) => t + v, 0) / size);
  }
  return out;
}

/** Least-squares fit over the value sequence; r2 says how much of the variance it explains. */
export function linearTrend(values) {
  const points = values.filter(isNum);
  if (points.length < 2) return null;
  const n = points.length;
  const meanX = (n - 1) / 2;
  const meanY = points.reduce((t, v) => t + v, 0) / n;
  let sxy = 0;
  let sxx = 0;
  let syy = 0;
  for (let i = 0; i < n; i += 1) {
    sxy += (i - meanX) * (points[i] - meanY);
    sxx += (i - meanX) ** 2;
    syy += (points[i] - meanY) ** 2;
  }
  const slope = sxx === 0 ? 0 : sxy / sxx;
  const intercept = meanY - slope * meanX;
  const r2 = sxx === 0 || syy === 0 ? 0 : (sxy * sxy) / (sxx * syy);
  const direction = slope > 0 ? 'up' : slope < 0 ? 'down' : 'flat';
  return { slope, intercept, r2, direction };
}

/** Tukey's fence: outside 1.5 IQR of the quartiles. */
export function outliersIqr(records, field) {
  const q1 = percentileField(records, field, 0.25);
  const q3 = percentileField(records, field, 0.75);
  if (q1 === null || q3 === null) return [];
  const iqr = q3 - q1;
  const low = q1 - 1.5 * iqr;
  const high = q3 + 1.5 * iqr;
  return records.filter((r) => isNum(r[field]) && (r[field] < low || r[field] > high));
}

export function zScores(records, field) {
  const mean = meanField(records, field);
  const sd = stddevField(records, field);
  if (mean === null || sd === null || sd === 0) return records.map(() => null);
  return records.map((r) => (isNum(r[field]) ? (r[field] - mean) / sd : null));
}

/** Pearson correlation. Returns null when either side has no variance. */
export function correlation(records, leftField, rightField) {
  const pairs = records
    .map((r) => [r[leftField], r[rightField]])
    .filter(([a, b]) => isNum(a) && isNum(b));
  if (pairs.length < 2) return null;
  const n = pairs.length;
  const meanA = pairs.reduce((t, [a]) => t + a, 0) / n;
  const meanB = pairs.reduce((t, [, b]) => t + b, 0) / n;
  let cov = 0;
  let varA = 0;
  let varB = 0;
  for (const [a, b] of pairs) {
    cov += (a - meanA) * (b - meanB);
    varA += (a - meanA) ** 2;
    varB += (b - meanB) ** 2;
  }
  if (varA === 0 || varB === 0) return null;
  return cov / Math.sqrt(varA * varB);
}

const pad = (n) => String(n).padStart(2, '0');

/** Bucket a date field into day, week (ISO-ish, Monday start) or month. */
export function timeBucket(records, dateField, bucket = 'day') {
  const buckets = new Map();
  for (const record of records) {
    const raw = record[dateField];
    const date = raw instanceof Date ? raw : new Date(String(raw));
    if (Number.isNaN(date.getTime())) continue;
    let key;
    if (bucket === 'month') {
      key = `${date.getUTCFullYear()}-${pad(date.getUTCMonth() + 1)}`;
    } else if (bucket === 'week') {
      const monday = new Date(Date.UTC(date.getUTCFullYear(), date.getUTCMonth(), date.getUTCDate()));
      const day = (monday.getUTCDay() + 6) % 7;
      monday.setUTCDate(monday.getUTCDate() - day);
      key = monday.toISOString().slice(0, 10);
    } else {
      key = date.toISOString().slice(0, 10);
    }
    buckets.set(key, (buckets.get(key) || 0) + 1);
  }
  return [...buckets.entries()]
    .map(([key, n]) => ({ bucket: key, count: n }))
    .sort((a, b) => byField(a.bucket, b.bucket));
}

/** Summarise every numeric field in one pass. */
export function describeAll(records) {
  const fields = new Set();
  for (const record of records) {
    for (const [key, value] of Object.entries(record)) {
      if (isNum(value)) fields.add(key);
    }
  }
  const out = {};
  for (const field of [...fields].sort()) {
    out[field] = {
      count: numbers(records, field).length,
      min: minField(records, field),
      max: maxField(records, field),
      mean: meanField(records, field),
      median: medianField(records, field),
      stddev: stddevField(records, field),
    };
  }
  return out;
}

export function toCsv(records) {
  if (!records.length) return '';
  const columns = [...new Set(records.flatMap((r) => Object.keys(r)))];
  const escape = (value) => {
    if (value === null || value === undefined) return '';
    const text = String(value);
    return /[",\n]/.test(text) ? `"${text.replace(/"/g, '""')}"` : text;
  };
  const lines = [columns.join(',')];
  for (const record of records) {
    lines.push(columns.map((column) => escape(record[column])).join(','));
  }
  return lines.join('\n');
}

/** Data-quality report: missing values, type mismatches and duplicate rows. */
export function dataQuality(records, schema = {}) {
  const issues = [];
  const rows = records.length;
  for (const [field, meta] of Object.entries(schema)) {
    let missing = 0;
    let wrongType = 0;
    for (const record of records) {
      const value = record[field];
      if (value === undefined || value === null || value === '') {
        missing += 1;
      } else if (meta.type === 'number' && !isNum(value)) {
        wrongType += 1;
      }
    }
    if (missing) issues.push({ field, kind: 'missing', count: missing, share: rows ? missing / rows : 0 });
    if (wrongType) issues.push({ field, kind: 'type', count: wrongType, share: rows ? wrongType / rows : 0 });
  }
  const seen = new Set();
  let duplicates = 0;
  for (const record of records) {
    const key = JSON.stringify(record);
    if (seen.has(key)) duplicates += 1;
    else seen.add(key);
  }
  if (duplicates) issues.push({ field: '*', kind: 'duplicate-row', count: duplicates, share: rows ? duplicates / rows : 0 });
  return { rows, fields: Object.keys(schema).length, issues, ok: issues.length === 0 };
}
