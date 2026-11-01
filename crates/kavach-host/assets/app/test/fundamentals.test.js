// One test per group of fundamentals, against a hand-computed fixture.
//
// The numbers below were worked out by hand from the fixture, not copied from the code's
// output — otherwise the test would only prove the code is consistent with itself.

import test from 'node:test';
import assert from 'node:assert/strict';

import {
  grossMargin, operatingMargin, netMargin, ebitdaMargin, roe, roa, roic, effectiveTaxRate,
  assetTurnover, inventoryTurnover, daysInventory, receivablesTurnover, daysSalesOutstanding,
  currentRatio, quickRatio, cashRatio, workingCapital,
  debtToEquity, debtToAssets, netDebt, interestCoverage,
  bookValuePerShare, epsBasic, peRatio, pbRatio, psRatio, dividendPayout,
  marketCap, enterpriseValue, evEbitda, freeCashFlow, fcfMargin, grahamNumber,
  dupont, altmanZ, piotroskiF, ruleOf40, qualityFlags, analyse, normaliseStatement,
} from '../src/fundamentals.js';

const near = (actual, expected, message) =>
  assert.ok(Math.abs(actual - expected) < 1e-9, `${message}: expected ${expected}, got ${actual}`);

// revenue 1000, gross 400, operating income 200, EBIT 190, interest 10, tax 45, net income 135.
const company = {
  revenue: 1000, costOfGoods: 600, grossProfit: 400,
  operatingIncome: 200, ebit: 190, interest: 10, tax: 45, preTaxIncome: 180, netIncome: 135,
  totalAssets: 800, totalLiabilities: 500, totalEquity: 300,
  currentAssets: 400, currentLiabilities: 200, inventory: 100, receivables: 150, cash: 50,
  debt: 200, longTermDebt: 150,
  operatingCashFlow: 160, capex: 60, depreciation: 30, dividends: 40,
  sharesOutstanding: 100, price: 20,
  priorRevenue: 900, priorNetIncome: 100, priorTotalAssets: 750,
  priorCurrentAssets: 350, priorCurrentLiabilities: 190, priorLongTermDebt: 170,
  priorSharesOutstanding: 100, priorGrossProfit: 320,
};

test('a lowercased CSV row normalises onto the canonical names', () => {
  // The pipeline lowercases and strips headers, so `totalEquity` arrives as `totalequity`.
  const row = {
    company: 'AAA', period: 2025, revenue: 1000, costofgoods: 600, operatingincome: 200,
    ebit: 190, interest: 10, tax: 45, pretaxincome: 180, netincome: 135,
    totalassets: 800, totalliabilities: 500, totalequity: 300,
    currentassets: 400, currentliabilities: 200, inventory: 100, receivables: 150,
    cash: 50, debt: 200, longtermdebt: 150, operatingcashflow: 160, capex: 60,
    depreciation: 30, dividends: 40, sharesoutstanding: 100, price: 20,
    priorrevenue: 900, priornetincome: 100, priortotalassets: 750,
  };
  const s = normaliseStatement(row);
  near(grossMargin(s), 0.4, 'gross margin after normalising');
  near(roe(s), 0.45, 'roe after normalising');
  near(roic(s), 0.285, 'roic after normalising');
  near(peRatio(s), 20 / 1.35, 'pe after normalising');
  assert.equal(s.priorRevenue, 900, 'prior fields normalise too');
  assert.ok(!('company' in s), 'non-financial columns are dropped');
});

test('normalisation accepts spaced headers and numeric strings', () => {
  const s = normaliseStatement({ 'Total Equity': '300', total_assets: '800', net_income: '135', revenue: '1,000' });
  assert.equal(s.totalEquity, 300);
  assert.equal(s.totalAssets, 800);
  assert.equal(s.revenue, 1000);
  near(roe(s), 0.45, 'roe from spaced headers');
});

test('normalisation rejects nonsense rather than guessing', () => {
  const s = normaliseStatement({ revenue: 'not a number', totalEquity: 300 });
  assert.equal(s.revenue, undefined);
  assert.equal(grossMargin(s), null);
  assert.deepEqual(normaliseStatement(null), {});
});

test('profitability ratios match hand calculation', () => {
  near(grossMargin(company), 0.4, 'gross margin');
  near(operatingMargin(company), 0.2, 'operating margin');
  near(netMargin(company), 0.135, 'net margin');
  near(ebitdaMargin(company), 0.23, 'ebitda margin');
  near(roe(company), 0.45, 'roe');
  near(roa(company), 0.16875, 'roa');
  // NOPAT = 190 x (1 - 0.25); invested = 300 equity + 200 debt = 500.
  near(roic(company), 0.285, 'roic');
  near(effectiveTaxRate(company), 0.25, 'tax rate');
});

test('gross profit is derived when only revenue and cost are given', () => {
  const s = { revenue: 500, costOfGoods: 300 };
  near(grossMargin(s), 0.4, 'derived gross margin');
});

test('efficiency ratios and day counts', () => {
  near(assetTurnover(company), 1.25, 'asset turnover');
  near(inventoryTurnover(company), 6, 'inventory turnover');
  near(daysInventory(company), 365 / 6, 'days inventory');
  near(receivablesTurnover(company), 1000 / 150, 'receivables turnover');
  near(daysSalesOutstanding(company), 365 / (1000 / 150), 'DSO');
});

test('liquidity ratios', () => {
  near(currentRatio(company), 2, 'current ratio');
  near(quickRatio(company), 1.5, 'quick ratio');
  near(cashRatio(company), 0.25, 'cash ratio');
  near(workingCapital(company), 200, 'working capital');
});

test('leverage ratios', () => {
  near(debtToEquity(company), 200 / 300, 'debt to equity');
  near(debtToAssets(company), 0.25, 'debt to assets');
  near(netDebt(company), 150, 'net debt');
  near(interestCoverage(company), 19, 'interest cover');
});

test('per-share and price ratios', () => {
  near(bookValuePerShare(company), 3, 'book value per share');
  near(epsBasic(company), 1.35, 'eps');
  near(peRatio(company), 20 / 1.35, 'pe');
  near(pbRatio(company), 20 / 3, 'pb');
  near(psRatio(company), 2, 'ps');
  near(dividendPayout(company), 40 / 135, 'payout');
});

test('valuation: market cap, enterprise value, FCF and Graham', () => {
  near(marketCap(company), 2000, 'market cap');
  near(enterpriseValue(company), 2150, 'enterprise value');
  near(evEbitda(company), 2150 / 230, 'ev/ebitda');
  near(freeCashFlow(company), 100, 'free cash flow');
  near(fcfMargin(company), 0.1, 'fcf margin');
  near(grahamNumber(company), Math.sqrt(22.5 * 1.35 * 3), 'graham number');
});

test('Graham number refuses a loss-maker', () => {
  assert.equal(grahamNumber({ netIncome: -50, totalEquity: 300, sharesOutstanding: 100 }), null);
});

test('DuPont multiplies back to ROE', () => {
  const d = dupont(company);
  near(d.margin, 0.135, 'dupont margin');
  near(d.turnover, 1.25, 'dupont turnover');
  near(d.leverage, 800 / 300, 'dupont leverage');
  near(d.roe, roe(company), 'dupont roe must equal roe');
});

test('Altman Z places a healthy company in the safe zone', () => {
  const result = altmanZ(company);
  near(result.z, 4.97, 'altman z');
  assert.equal(result.zone, 'safe');
  assert.equal(result.retainedEarningsProxy, true, 'retained earnings were not reported');
});

test('Altman Z flags distress', () => {
  const weak = {
    revenue: 100, netIncome: -20, ebit: -10, totalAssets: 500, totalLiabilities: 480,
    currentAssets: 50, currentLiabilities: 120, debt: 470, sharesOutstanding: 100, price: 1,
  };
  const result = altmanZ(weak);
  assert.equal(result.zone, 'distress', `z was ${result.z}`);
});

test('Piotroski scores all nine signals for an improving company', () => {
  const result = piotroskiF(company);
  assert.equal(result.score, 9, `signals: ${JSON.stringify(result.signals)}`);
  assert.equal(result.of, 9);
});

test('Piotroski scores low for a deteriorating company', () => {
  const weak = {
    revenue: 800, netIncome: -10, operatingCashFlow: -30, totalAssets: 900,
    totalEquity: 200, currentAssets: 200, currentLiabilities: 250,
    longTermDebt: 400, sharesOutstanding: 130, grossProfit: 150,
    priorRevenue: 900, priorNetIncome: 50, priorTotalAssets: 800,
    priorCurrentAssets: 300, priorCurrentLiabilities: 200, priorLongTermDebt: 300,
    priorSharesOutstanding: 100, priorGrossProfit: 250,
  };
  assert.ok(piotroskiF(weak).score <= 2, 'a deteriorating company should not score well');
});

test('Rule of 40 combines growth and margin', () => {
  const result = ruleOf40(company);
  near(result.growth, 100 / 900, 'growth');
  near(result.total, 100 / 900 + 0.1, 'rule of 40 total');
  assert.equal(result.passes, false, '21% does not pass the 40 threshold');

  const strong = { revenue: 1500, priorRevenue: 1000, operatingCashFlow: 400, capex: 100 };
  assert.equal(ruleOf40(strong).passes, true);
});

test('quality flags fire on the problems they describe', () => {
  assert.deepEqual(qualityFlags(company), [], 'a clean company raises no flags');

  const weak = {
    revenue: 1000, netIncome: 100, operatingCashFlow: 20, capex: 80,
    totalAssets: 800, totalEquity: 100, debt: 400, currentAssets: 150, currentLiabilities: 200,
    ebit: 10, interest: 12, sharesOutstanding: 120, priorSharesOutstanding: 100,
    priorGrossProfit: 400, priorRevenue: 1000, grossProfit: 300,
  };
  const flags = qualityFlags(weak).map((f) => f.flag);
  for (const expected of [
    'profit-not-backing-with-cash', 'negative-free-cash-flow', 'high-leverage',
    'liquidity-below-one', 'margin-compression', 'dilution', 'thin-interest-cover',
  ]) {
    assert.ok(flags.includes(expected), `expected ${expected} in ${flags.join(', ')}`);
  }
});

test('missing inputs return null, never NaN or Infinity', () => {
  const empty = {};
  for (const fn of [grossMargin, operatingMargin, netMargin, roe, roa, roic, currentRatio,
    quickRatio, debtToEquity, interestCoverage, peRatio, pbRatio, psRatio, evEbitda,
    fcfMargin, grahamNumber]) {
    const value = fn(empty);
    assert.equal(value, null, `${fn.name} should be null for an empty statement`);
    assert.ok(!Number.isNaN(value), `${fn.name} must not return NaN`);
  }
});

test('a zero denominator is null, not Infinity', () => {
  assert.equal(roe({ netIncome: 100, totalEquity: 0 }), null);
  assert.equal(currentRatio({ currentAssets: 100, currentLiabilities: 0 }), null);
  assert.equal(peRatio({ price: 20, netIncome: 0, sharesOutstanding: 100 }), null);
  assert.equal(daysInventory({ costOfGoods: 100, inventory: 0 }), null);
});

test('analyse returns every group and stays serialisable', () => {
  const report = analyse(company);
  for (const group of ['profitability', 'efficiency', 'liquidity', 'leverage', 'perShare', 'valuation', 'composites', 'flags']) {
    assert.ok(group in report, `missing group ${group}`);
  }
  const json = JSON.stringify(report);
  assert.ok(!json.includes('NaN'), 'the report must not contain NaN');
  assert.ok(!json.includes('Infinity'), 'the report must not contain Infinity');
  near(report.profitability.roe, 0.45, 'nested roe');
});
