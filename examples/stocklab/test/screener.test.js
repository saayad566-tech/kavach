// Tests for screening, ranking, peer scoring and DCF.
//
// The DCF expectations are computed by hand in the comments, so a regression shows up as a
// number that no longer matches the arithmetic rather than as a changed snapshot.

import test from 'node:test';
import assert from 'node:assert/strict';

import {
  screen, rank, percentileRank, median, withPercentile, sectorSummary, peerScore, rankByPeerScore, metricOf,
} from '../src/screener.js';
import { dcf, sensitivity, marginOfSafety, impliedGrowth } from '../src/dcf.js';

const companies = [
  { symbol: 'AAA', name: 'Alpha', sector: 'Tech', metrics: { roe: 0.45, roic: 0.28, netMargin: 0.135, debtToEquity: 0.67, peRatio: 14.8 } },
  { symbol: 'BBB', name: 'Beta', sector: 'Tech', metrics: { roe: 0.30, roic: 0.20, netMargin: 0.09, debtToEquity: 2.0, peRatio: 8.0 } },
  { symbol: 'CCC', name: 'Gamma', sector: 'Energy', metrics: { roe: 0.12, roic: 0.08, netMargin: 0.05, debtToEquity: 1.2, peRatio: 6.0 } },
  { symbol: 'DDD', name: 'Delta', sector: 'Energy', metrics: { roe: 0.20, roic: 0.15, netMargin: 0.11, debtToEquity: 0.9, peRatio: 11.0 } },
  { symbol: 'EEE', name: 'Epsilon', sector: 'Tech', metrics: { roe: null } },
];

test('screening separates pass, fail and excluded', () => {
  const result = screen(companies, [{ metric: 'roe', op: 'gt', value: 0.2 }]);
  assert.deepEqual(result.passed.map((c) => c.symbol), ['AAA', 'BBB']);
  // DDD sits exactly on 0.20, and the rule is strictly greater, so it fails rather than passes.
  assert.deepEqual(result.failed.map((c) => c.symbol), ['CCC', 'DDD']);
  // EEE has no ROE at all: excluded, which is a different thing from failing.
  assert.deepEqual(result.excluded.map((c) => c.symbol), ['EEE']);
  assert.equal(result.total, 5);
});

test('a company missing the metric is excluded, never treated as zero', () => {
  const result = screen(companies, [{ metric: 'roe', op: 'gte', value: 0 }]);
  assert.ok(!result.passed.some((c) => c.symbol === 'EEE'), 'EEE has no ROE and must not pass a "roe >= 0" screen');
  assert.ok(result.excluded.some((c) => c.symbol === 'EEE'));
});

test('multiple criteria are applied together', () => {
  const result = screen(companies, [
    { metric: 'roe', op: 'gt', value: 0.15 },
    { metric: 'debtToEquity', op: 'lt', value: 1.5 },
  ]);
  assert.deepEqual(result.passed.map((c) => c.symbol), ['AAA', 'DDD']);
});

test('an unknown operator throws rather than silently passing everything', () => {
  assert.throws(() => screen(companies, [{ metric: 'roe', op: 'greaterish', value: 1 }]), /unknown operator/);
});

test('ranking is ordered and puts missing values last', () => {
  assert.deepEqual(rank(companies, 'roe').map((c) => c.symbol), ['AAA', 'BBB', 'DDD', 'CCC', 'EEE']);
  assert.deepEqual(rank(companies, 'roe', 'asc').map((c) => c.symbol), ['CCC', 'DDD', 'BBB', 'AAA', 'EEE']);
});

test('percentile rank handles ties and empty lists', () => {
  assert.equal(percentileRank(5, [1, 2, 3, 4, 5]), 0.9);
  assert.equal(percentileRank(1, [1, 1, 1]), 0.5);
  assert.equal(percentileRank(5, []), null);
  assert.equal(percentileRank(null, [1, 2]), null);
});

test('median resists one outlier', () => {
  assert.equal(median([1, 2, 3, 100]), 2.5);
  assert.equal(median([4]), 4);
  assert.equal(median([]), null);
});

test('withPercentile annotates each company', () => {
  const annotated = withPercentile(companies, 'roe');
  assert.equal(annotated.find((c) => c.symbol === 'EEE').percentile, null);
  assert.ok(annotated.find((c) => c.symbol === 'AAA').percentile > 0.8);
});

test('sector summary groups and reports medians', () => {
  const summary = sectorSummary(companies, 'roe');
  const tech = summary.find((s) => s.sector === 'Tech');
  assert.equal(tech.count, 3);
  assert.equal(tech.measured, 2, 'EEE has no ROE and is not counted as measured');
  assert.equal(tech.median, (0.45 + 0.30) / 2);
  assert.equal(tech.best, 0.45);
});

test('peer score is relative to the sector, not the whole market', () => {
  // AAA has the best ROE in Tech; CCC the worst in Energy. Both should score well or badly
  // relative to their own peers.
  const scores = peerScore(companies, ['roe']);
  const aaa = scores.find((s) => s.symbol === 'AAA');
  const ccc = scores.find((s) => s.symbol === 'CCC');
  // Mid-rank percentile: in a two-company sector the leader scores 0.75, not 1.
  assert.ok(aaa.score > 0.7, `AAA peer score ${aaa.score}`);
  assert.ok(ccc.score < 0.5, `CCC peer score ${ccc.score}`);
  assert.equal(scores.find((s) => s.symbol === 'EEE').score, null, 'no metrics means no score');
});

test('lower-is-better metrics are inverted in the peer score', () => {
  // BBB has the highest debt/equity in Tech and should score worst on that metric alone.
  const scores = peerScore(companies, ['debtToEquity']);
  const bbb = scores.find((s) => s.symbol === 'BBB');
  const aaa = scores.find((s) => s.symbol === 'AAA');
  assert.ok(aaa.score > bbb.score, 'lower debt must score better');
});

test('ranking by peer score is ordered and handles nulls', () => {
  const ranked = rankByPeerScore(companies, ['roe', 'roic']);
  assert.equal(ranked[0].symbol, 'AAA');
  assert.equal(ranked[ranked.length - 1].symbol, 'EEE');
});

test('DCF matches hand arithmetic', () => {
  // FCF 100, growth 0 for simplicity, discount 10%, terminal growth 0%, 3 years, 100 shares.
  const result = dcf({ freeCashFlow: 100, growth: 0, discount: 0.10, terminalGrowth: 0, years: 3, shares: 100 });
  // PV of 100 for 3 years at 10% = 90.909 + 82.645 + 75.131 = 248.685
  assert.ok(Math.abs(result.presentValueOfForecast - 248.685) < 0.01, `got ${result.presentValueOfForecast}`);
  // Terminal = 100 / 0.10 = 1000, discounted 3 years = 751.315
  assert.ok(Math.abs(result.presentValueOfTerminal - 751.315) < 0.01, `got ${result.presentValueOfTerminal}`);
  assert.ok(Math.abs(result.enterpriseValue - 1000) < 0.01, `got ${result.enterpriseValue}`);
  assert.ok(Math.abs(result.perShare - 10) < 0.01, `got ${result.perShare}`);
});

test('DCF refuses an infinite perpetuity', () => {
  const result = dcf({ freeCashFlow: 100, growth: 0.05, discount: 0.05, terminalGrowth: 0.05, years: 10 });
  assert.ok(result.error, 'terminal growth at the discount rate must be refused');
});

test('DCF subtracts net debt to reach equity value', () => {
  const base = dcf({ freeCashFlow: 100, growth: 0, discount: 0.10, terminalGrowth: 0, years: 5, shares: 100 });
  const levered = dcf({ freeCashFlow: 100, growth: 0, discount: 0.10, terminalGrowth: 0, years: 5, shares: 100, netDebt: 500 });
  assert.ok(Math.abs((base.equityValue - levered.equityValue) - 500) < 0.01);
  assert.ok(levered.perShare < base.perShare);
});

test('DCF returns null for unusable inputs', () => {
  assert.equal(dcf({ freeCashFlow: null, growth: 0, discount: 0.1, terminalGrowth: 0 }), null);
  assert.equal(dcf({ freeCashFlow: 100, growth: 0, discount: 0.1, terminalGrowth: 0, years: 0 }), null);
});

test('the sensitivity grid brackets the base case', () => {
  const inputs = { freeCashFlow: 100, growth: 0.05, discount: 0.10, terminalGrowth: 0.02, years: 10, shares: 100 };
  const grid = sensitivity(inputs);
  assert.equal(grid.rows.length, 5);
  assert.equal(grid.rows[0].cells.length, 5);
  assert.ok(grid.spread > 0, 'a DCF must show a range, not a point');
  const base = dcf(inputs).perShare;
  assert.ok(base >= grid.min - 1e-9 && base <= grid.max + 1e-9, 'the base case lies inside its own grid');
});

test('margin of safety is positive only below the estimate', () => {
  assert.ok(Math.abs(marginOfSafety(100, 70) - 0.3) < 1e-9);
  assert.ok(marginOfSafety(100, 130) < 0);
  assert.equal(marginOfSafety(0, 10), null);
});

test('implied growth recovers the growth that justifies a price', () => {
  const inputs = { freeCashFlow: 100, growth: 0.08, discount: 0.10, terminalGrowth: 0.02, years: 10, shares: 100 };
  const value = dcf(inputs).perShare;
  const recovered = impliedGrowth({ ...inputs, growth: 0 }, value);
  assert.ok(Math.abs(recovered - 0.08) < 0.005, `expected about 0.08, got ${recovered}`);
});

test('metricOf reads metrics then top-level fields', () => {
  assert.equal(metricOf({ metrics: { roe: 1 } }, 'roe'), 1);
  assert.equal(metricOf({ roe: 2 }, 'roe'), 2);
  assert.equal(metricOf({}, 'roe'), null);
  assert.equal(metricOf(null, 'roe'), null);
});
