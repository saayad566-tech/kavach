// One test per analysis feature. If a function here is wrong, a generated application is
// wrong, so each feature earns its own assertion.

import test from 'node:test';
import assert from 'node:assert/strict';

import {
  count, sumField, meanField, medianField, modeField, minField, maxField,
  varianceField, stddevField, percentileField, frequency, distinctCount,
  groupAggregate, topN, sortBy, filterWhere, paginate, growthRate, cagr,
  movingAverage, linearTrend, outliersIqr, zScores, correlation, timeBucket,
  describeAll, toCsv, dataQuality,
} from '../src/analytics.js';

const rows = [
  { date: '2026-01-05', symbol: 'AAA', close: 10, volume: 100 },
  { date: '2026-01-06', symbol: 'AAA', close: 12, volume: 120 },
  { date: '2026-01-07', symbol: 'BBB', close: 14, volume: 140 },
  { date: '2026-01-08', symbol: 'BBB', close: 16, volume: 160 },
  { date: '2026-01-09', symbol: 'AAA', close: 20, volume: 200 },
];

test('count', () => {
  assert.equal(count(rows), 5);
  assert.equal(count([]), 0);
  assert.equal(count(null), 0);
});

test('sum, mean, min, max', () => {
  assert.equal(sumField(rows, 'close'), 72);
  assert.equal(meanField(rows, 'close'), 14.4);
  assert.equal(minField(rows, 'close'), 10);
  assert.equal(maxField(rows, 'close'), 20);
});

test('median handles even and odd counts', () => {
  assert.equal(medianField(rows, 'close'), 14);
  assert.equal(medianField(rows.slice(0, 4), 'close'), 13);
});

test('mode picks the most frequent value', () => {
  assert.equal(modeField(rows, 'symbol'), 'AAA');
});

test('variance and stddev use the sample formula', () => {
  assert.ok(Math.abs(varianceField(rows, 'close') - 14.8) < 1e-9);
  assert.ok(Math.abs(stddevField(rows, 'close') - Math.sqrt(14.8)) < 1e-9);
});

test('percentiles are nearest-rank', () => {
  assert.equal(percentileField(rows, 'close', 0), 10);
  assert.equal(percentileField(rows, 'close', 1), 20);
  assert.equal(percentileField(rows, 'close', 0.5), 14);
  assert.equal(percentileField(rows, 'close', 2), null, 'out-of-range p returns null');
});

test('frequency and distinct count', () => {
  const counts = frequency(rows, 'symbol');
  assert.equal(counts[0].value, 'AAA');
  assert.equal(counts[0].count, 3);
  assert.equal(distinctCount(rows, 'symbol'), 2);
});

test('groupAggregate supports every documented aggregate', () => {
  assert.deepEqual(groupAggregate(rows, 'symbol', 'close', 'sum'), [
    { key: 'AAA', value: 42, n: 3 },
    { key: 'BBB', value: 30, n: 2 },
  ]);
  // Results are ordered by aggregate value, so look groups up by key rather than by index.
  const byKey = (how) => Object.fromEntries(groupAggregate(rows, 'symbol', 'close', how).map((g) => [g.key, g.value]));
  assert.equal(byKey('mean').AAA, 14);
  assert.equal(byKey('max').BBB, 16);
  assert.equal(byKey('count').AAA, 3);
  assert.throws(() => groupAggregate(rows, 'symbol', 'close', 'median'), /unknown aggregate/);
});

test('topN sorts by direction and clamps', () => {
  assert.deepEqual(topN(rows, 'close', 2).map((r) => r.close), [20, 16]);
  assert.deepEqual(topN(rows, 'close', 2, 'asc').map((r) => r.close), [10, 12]);
  assert.equal(topN(rows, 'close', 99).length, 5);
});

test('sortBy is stable across types', () => {
  assert.deepEqual(sortBy(rows, 'close', 'desc').map((r) => r.close), [20, 16, 14, 12, 10]);
  assert.equal(sortBy(rows, 'symbol')[0].symbol, 'AAA');
});

test('filterWhere supports each operator', () => {
  assert.equal(filterWhere(rows, { field: 'close', op: 'gt', value: 15 }).length, 2);
  assert.equal(filterWhere(rows, { field: 'close', op: 'gte', value: 14 }).length, 3);
  assert.equal(filterWhere(rows, { field: 'symbol', op: 'eq', value: 'BBB' }).length, 2);
  assert.equal(filterWhere(rows, { field: 'symbol', op: 'in', value: ['AAA'] }).length, 3);
  assert.equal(filterWhere(rows, { field: 'symbol', op: 'contains', value: 'bb' }).length, 2);
  assert.throws(() => filterWhere(rows, { field: 'x', op: 'nope', value: 1 }), /unknown operator/);
});

test('paginate reports pages and slices', () => {
  const page = paginate(rows, 2, 2);
  assert.equal(page.items.length, 2);
  assert.equal(page.items[0].close, 14);
  assert.equal(page.total, 5);
  assert.equal(page.pages, 3);
});

test('growthRate is period over period', () => {
  const series = [
    { d: '2026-01-01', v: 100 },
    { d: '2026-01-02', v: 110 },
    { d: '2026-01-03', v: 121 },
  ];
  const rates = growthRate(series, 'd', 'v');
  assert.equal(rates.length, 2);
  assert.ok(Math.abs(rates[0].change - 0.1) < 1e-12);
  assert.ok(Math.abs(rates[1].change - 0.1) < 1e-12);
});

test('growthRate guards a zero base', () => {
  const rates = growthRate([{ d: 'a', v: 0 }, { d: 'b', v: 5 }], 'd', 'v');
  assert.equal(rates[0].change, null);
});

test('cagr compounds correctly', () => {
  assert.ok(Math.abs(cagr(100, 121, 2) - 0.1) < 1e-12);
  assert.equal(cagr(0, 100, 2), null);
  assert.equal(cagr(100, 121, 0), null);
});

test('movingAverage windows and handles gaps', () => {
  assert.deepEqual(movingAverage([1, 2, 3, 4], 2), [1.5, 2.5, 3.5]);
  assert.deepEqual(movingAverage([1, 2], 3), []);
  assert.deepEqual(movingAverage([1, null, 3], 2), [null, null]);
});

test('linearTrend reports slope, r2 and direction', () => {
  const trend = linearTrend([10, 12, 14, 16]);
  assert.ok(Math.abs(trend.slope - 2) < 1e-12);
  assert.ok(Math.abs(trend.r2 - 1) < 1e-12);
  assert.equal(trend.direction, 'up');
  assert.equal(linearTrend([5]), null);
});

test('outliersIqr finds the far value', () => {
  const data = [1, 2, 3, 4, 5, 6, 7, 8, 9, 100].map((close) => ({ close }));
  const found = outliersIqr(data, 'close');
  assert.equal(found.length, 1);
  assert.equal(found[0].close, 100);
});

test('zScores are standard deviations from the mean', () => {
  const scores = zScores(rows, 'close');
  assert.equal(scores.length, 5);
  assert.ok(Math.abs(scores[0] - (10 - 14.4) / Math.sqrt(14.8)) < 1e-12);
});

test('correlation is 1 for a perfect line and null without variance', () => {
  const pairs = [{ a: 1, b: 2 }, { a: 2, b: 4 }, { a: 3, b: 6 }];
  assert.ok(Math.abs(correlation(pairs, 'a', 'b') - 1) < 1e-12);
  assert.equal(correlation([{ a: 1, b: 1 }, { a: 1, b: 2 }], 'a', 'b'), null);
});

test('timeBucket groups by day, week and month', () => {
  assert.equal(timeBucket(rows, 'date', 'day').length, 5);
  const months = timeBucket(rows, 'date', 'month');
  assert.deepEqual(months, [{ bucket: '2026-01', count: 5 }]);
  const weeks = timeBucket(rows, 'date', 'week');
  assert.ok(weeks.length >= 1 && weeks.length <= 2, `got ${weeks.length}`);
});

test('describeAll summarises every numeric field', () => {
  const summary = describeAll(rows);
  assert.deepEqual(Object.keys(summary).sort(), ['close', 'volume']);
  assert.equal(summary.close.count, 5);
  assert.equal(summary.close.max, 20);
});

test('toCsv round-trips and quotes properly', () => {
  const csv = toCsv([{ a: 1, b: 'x,y' }, { a: 2, b: 'say "hi"' }]);
  assert.equal(csv, 'a,b\n1,"x,y"\n2,"say ""hi"""');
});

test('dataQuality reports missing, wrong type and duplicates', () => {
  const records = [
    { name: 'a', amount: 1 },
    { name: 'a', amount: 1 },
    { name: '', amount: 'nope' },
  ];
  const report = dataQuality(records, { name: { type: 'string' }, amount: { type: 'number' } });
  assert.equal(report.rows, 3);
  assert.equal(report.ok, false);
  const kinds = report.issues.map((i) => i.kind);
  assert.ok(kinds.includes('missing'), kinds.join(','));
  assert.ok(kinds.includes('type'), kinds.join(','));
  assert.ok(kinds.includes('duplicate-row'), kinds.join(','));
});

test('an empty dataset yields nulls, never NaN', () => {
  for (const fn of [meanField, medianField, minField, maxField, varianceField, stddevField]) {
    const value = fn([], 'x');
    assert.equal(value, null, `${fn.name} should return null`);
    assert.ok(!Number.isNaN(value), `${fn.name} must not return NaN`);
  }
});
