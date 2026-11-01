// Fundamental analysis for a generated Kavach application.
//
// Everything here is a pure function over one period of financial statements. That is what
// makes it testable, and what keeps the promise that a generated app does its analysis on this
// machine with data the user supplies — no feed, no account, no quota.
//
// Conventions, applied without exception:
//   * a ratio whose denominator is zero or missing returns `null`, never Infinity or NaN;
//   * money is in whole units of one currency, shares in whole shares;
//   * `prior*` fields hold the previous period, and anything needing two periods returns null
//     when they are absent rather than pretending the business did not change.
//
// The formulas are the standard ones; where a convention is a choice (Altman's variant,
// whether ROIC uses total or invested capital) the choice is stated in the comment.

const isNum = (v) => typeof v === 'number' && Number.isFinite(v);

/** Safe division: null when either side is missing or the denominator is zero. */
const div = (a, b) => (isNum(a) && isNum(b) && b !== 0 ? a / b : null);
const sum = (...values) => (values.every(isNum) ? values.reduce((t, v) => t + v, 0) : null);
const sub = (a, b) => (isNum(a) && isNum(b) ? a - b : null);

// ---------------------------------------------------------------- input normalisation

/**
 * The canonical field names these functions read.
 *
 * A CSV header arrives lowercased and stripped by the pipeline (`totalEquity` becomes
 * `totalequity`), and a hand-written file may use `total_equity` or `Total Equity`. Rather
 * than make every ratio tolerant, records are normalised once at the boundary — which is also
 * where a numeric string becomes a number.
 */
const CANONICAL = [
  'revenue', 'costOfGoods', 'grossProfit', 'operatingIncome', 'ebit', 'ebitda', 'interest',
  'tax', 'preTaxIncome', 'netIncome', 'totalAssets', 'totalLiabilities', 'totalEquity',
  'currentAssets', 'currentLiabilities', 'inventory', 'receivables', 'cash', 'debt',
  'shortTermDebt', 'longTermDebt', 'retainedEarnings', 'operatingCashFlow', 'capex',
  'depreciation', 'dividends', 'sharesOutstanding', 'price', 'eps', 'marketCapValue',
];

const PRIOR = ['revenue', 'netIncome', 'totalAssets', 'currentAssets', 'currentLiabilities',
  'longTermDebt', 'sharesOutstanding', 'grossProfit'];

const flatten = (key) => key.toLowerCase().replace(/[^a-z0-9]/g, '');

const toNumber = (value) => {
  if (isNum(value)) return value;
  if (typeof value === 'string' && value.trim() !== '') {
    const parsed = Number(value.replace(/,/g, ''));
    return Number.isFinite(parsed) ? parsed : undefined;
  }
  return undefined;
};

/**
 * Map any reasonable spelling of a statement onto the canonical field names.
 *
 * Unknown fields are dropped rather than carried through: a ratio that silently read the wrong
 * column would be worse than one that reports "no data".
 */
export function normaliseStatement(raw) {
  if (!raw || typeof raw !== 'object') return {};
  const lookup = new Map();
  for (const [key, value] of Object.entries(raw)) {
    lookup.set(flatten(key), value);
  }

  const out = {};
  for (const key of CANONICAL) {
    const value = toNumber(lookup.get(flatten(key)));
    if (value !== undefined) out[key] = value;
  }
  for (const key of PRIOR) {
    const priorKey = 'prior' + key.charAt(0).toUpperCase() + key.slice(1);
    const value = toNumber(lookup.get(flatten(priorKey)));
    if (value !== undefined) out[priorKey] = value;
  }
  return out;
}

// ---------------------------------------------------------------- profitability

/** Gross margin: how much of each unit of revenue survives the cost of producing it. */
export function grossMargin(s) {
  const gross = isNum(s.grossProfit) ? s.grossProfit : sub(s.revenue, s.costOfGoods);
  return div(gross, s.revenue);
}

export function operatingMargin(s) {
  return div(s.operatingIncome, s.revenue);
}

export function netMargin(s) {
  return div(s.netIncome, s.revenue);
}

/** EBITDA margin, using the reported EBITDA or deriving it from operating income. */
export function ebitdaMargin(s) {
  const ebitda = isNum(s.ebitda) ? s.ebitda : sum(s.operatingIncome, s.depreciation);
  return div(ebitda, s.revenue);
}

/** Return on equity: profit per unit of shareholders' money. */
export function roe(s) {
  return div(s.netIncome, s.totalEquity);
}

/** Return on assets: profit per unit of everything the business owns. */
export function roa(s) {
  return div(s.netIncome, s.totalAssets);
}

/**
 * Return on invested capital.
 *
 * Invested capital is equity plus total debt, which is the conservative definition: it charges
 * the company for borrowed money as well as shareholders' money. NOPAT uses the effective tax
 * rate when it can be derived, and the reported net income otherwise.
 */
export function roic(s) {
  const taxRate = effectiveTaxRate(s);
  const nopat = isNum(s.ebit) && isNum(taxRate) ? s.ebit * (1 - taxRate) : s.netIncome;
  const invested = sum(s.totalEquity, s.debt);
  return div(nopat, invested);
}

/** Effective tax rate, from the reported tax charge if present, else from pre-tax income. */
export function effectiveTaxRate(s) {
  if (isNum(s.tax) && isNum(s.preTaxIncome) && s.preTaxIncome !== 0) {
    return s.tax / s.preTaxIncome;
  }
  if (isNum(s.tax) && isNum(s.netIncome)) {
    const preTax = s.netIncome + s.tax;
    return div(s.tax, preTax);
  }
  return null;
}

// ---------------------------------------------------------------- efficiency

/** Revenue generated per unit of assets: how hard the balance sheet works. */
export function assetTurnover(s) {
  return div(s.revenue, s.totalAssets);
}

export function inventoryTurnover(s) {
  const cogs = isNum(s.costOfGoods) ? s.costOfGoods : sub(s.revenue, s.grossProfit);
  return div(cogs, s.inventory);
}

/** Days of inventory held: 365 divided by turnover. */
export function daysInventory(s) {
  const turns = inventoryTurnover(s);
  return isNum(turns) && turns !== 0 ? 365 / turns : null;
}

export function receivablesTurnover(s) {
  return div(s.revenue, s.receivables);
}

/** Days sales outstanding: how long cash takes to arrive. */
export function daysSalesOutstanding(s) {
  const turns = receivablesTurnover(s);
  return isNum(turns) && turns !== 0 ? 365 / turns : null;
}

// ---------------------------------------------------------------- liquidity

export function currentRatio(s) {
  return div(s.currentAssets, s.currentLiabilities);
}

/** Quick ratio: current assets less inventory, over current liabilities. */
export function quickRatio(s) {
  const quick = sub(s.currentAssets, isNum(s.inventory) ? s.inventory : 0);
  return div(quick, s.currentLiabilities);
}

export function cashRatio(s) {
  return div(s.cash, s.currentLiabilities);
}

export function workingCapital(s) {
  return sub(s.currentAssets, s.currentLiabilities);
}

// ---------------------------------------------------------------- leverage

export function debtToEquity(s) {
  return div(s.debt, s.totalEquity);
}

export function debtToAssets(s) {
  return div(s.debt, s.totalAssets);
}

/** Net debt: what is owed once cash is set against it. Negative means net cash. */
export function netDebt(s) {
  return sub(s.debt, isNum(s.cash) ? s.cash : 0);
}

/** Interest cover: how many times operating profit pays the interest bill. */
export function interestCoverage(s) {
  return div(s.ebit, s.interest);
}

// ---------------------------------------------------------------- per share

export function bookValuePerShare(s) {
  return div(s.totalEquity, s.sharesOutstanding);
}

export function epsBasic(s) {
  return isNum(s.eps) ? s.eps : div(s.netIncome, s.sharesOutstanding);
}

export function peRatio(s) {
  const earnings = epsBasic(s);
  return div(s.price, earnings);
}

export function pbRatio(s) {
  return div(s.price, bookValuePerShare(s));
}

export function psRatio(s) {
  const perShareRevenue = div(s.revenue, s.sharesOutstanding);
  return div(s.price, perShareRevenue);
}

/** Dividend payout ratio: the share of earnings paid out. */
export function dividendPayout(s) {
  return div(s.dividends, s.netIncome);
}

// ---------------------------------------------------------------- valuation

export function marketCap(s) {
  return isNum(s.marketCapValue) ? s.marketCapValue : (isNum(s.price) && isNum(s.sharesOutstanding) ? s.price * s.sharesOutstanding : null);
}

/** Enterprise value: what buying the whole business would cost. */
export function enterpriseValue(s) {
  const cap = marketCap(s);
  const debt = isNum(s.debt) ? s.debt : 0;
  const cash = isNum(s.cash) ? s.cash : 0;
  return isNum(cap) ? cap + debt - cash : null;
}

export function evEbitda(s) {
  const ebitda = isNum(s.ebitda) ? s.ebitda : sum(s.operatingIncome, s.depreciation);
  return div(enterpriseValue(s), ebitda);
}

/** Free cash flow: what the business generates after keeping itself running. */
export function freeCashFlow(s) {
  return sub(s.operatingCashFlow, s.capex);
}

export function fcfMargin(s) {
  return div(freeCashFlow(s), s.revenue);
}

/**
 * Graham number: an upper bound on a defensive investor's price.
 *
 * sqrt(22.5 x earnings per share x book value per share). Undefined for loss-makers, which is
 * the point — it is not a valuation for a company that does not earn.
 */
export function grahamNumber(s) {
  const eps = epsBasic(s);
  const bvps = bookValuePerShare(s);
  if (!isNum(eps) || !isNum(bvps) || eps <= 0 || bvps <= 0) return null;
  return Math.sqrt(22.5 * eps * bvps);
}

// ---------------------------------------------------------------- composites

/** Three-factor DuPont: margin x turnover x leverage = ROE. */
export function dupont(s) {
  const margin = netMargin(s);
  const turnover = assetTurnover(s);
  const leverage = div(s.totalAssets, s.totalEquity);
  if (!isNum(margin) || !isNum(turnover) || !isNum(leverage)) return null;
  return { margin, turnover, leverage, roe: margin * turnover * leverage };
}

/**
 * Altman Z-score, public-manufacturer variant.
 *
 *   Z = 1.2(WC/TA) + 1.4(RE/TA) + 3.3(EBIT/TA) + 0.6(MVE/TL) + 1.0(Sales/TA)
 *
 * Retained earnings are used when reported; net income stands in otherwise, which is a weaker
 * signal and is why the result carries `retainedEarningsProxy: true`.
 */
export function altmanZ(s) {
  const ta = s.totalAssets;
  if (!isNum(ta) || ta === 0) return null;
  const wc = workingCapital(s);
  const retained = isNum(s.retainedEarnings) ? s.retainedEarnings : s.netIncome;
  const mve = marketCap(s);
  const tl = isNum(s.totalLiabilities) ? s.totalLiabilities : s.debt;
  if (!isNum(wc) || !isNum(retained) || !isNum(s.ebit) || !isNum(mve) || !isNum(tl) || !isNum(s.revenue)) {
    return null;
  }
  const z =
    1.2 * (wc / ta) +
    1.4 * (retained / ta) +
    3.3 * (s.ebit / ta) +
    0.6 * (mve / tl) +
    1.0 * (s.revenue / ta);
  const zone = z > 2.99 ? 'safe' : z < 1.81 ? 'distress' : 'grey';
  return { z, zone, retainedEarningsProxy: !isNum(s.retainedEarnings) };
}

/**
 * Piotroski F-score: nine binary quality signals, 0-9.
 *
 * Needs two periods. Signals 1-4 are profitability, 5-7 funding, 8-9 operating efficiency.
 */
export function piotroskiF(s) {
  const signals = [];
  const prior = {
    roa: div(s.priorNetIncome, s.priorTotalAssets),
    leverage: div(s.priorLongTermDebt, s.priorTotalAssets),
    currentRatio: div(s.priorCurrentAssets, s.priorCurrentLiabilities),
    // Derive the prior gross profit when it was not reported: revenue less cost of goods
    // carries the same information, and a signal that fails only because a column is missing
    // understates quality.
    grossMargin: div(
      isNum(s.priorGrossProfit) ? s.priorGrossProfit : sub(s.priorRevenue, s.priorCostOfGoods),
      s.priorRevenue,
    ),
    assetTurnover: div(s.priorRevenue, s.priorTotalAssets),
  };
  const currentRoa = roa(s);
  const currentLeverage = div(s.longTermDebt, s.totalAssets);
  const currentRatioNow = currentRatio(s);
  const grossMarginNow = grossMargin(s);
  const turnoverNow = assetTurnover(s);

  signals.push(isNum(s.netIncome) && s.netIncome > 0);                       // 1 profitable
  signals.push(isNum(s.operatingCashFlow) && s.operatingCashFlow > 0);       // 2 cash positive
  signals.push(isNum(currentRoa) && isNum(prior.roa) && currentRoa > prior.roa); // 3 ROA up
  signals.push(isNum(s.operatingCashFlow) && isNum(s.netIncome) && s.operatingCashFlow > s.netIncome); // 4 accruals
  signals.push(isNum(currentLeverage) && isNum(prior.leverage) && currentLeverage < prior.leverage);   // 5 leverage down
  signals.push(isNum(currentRatioNow) && isNum(prior.currentRatio) && currentRatioNow > prior.currentRatio); // 6 liquidity up
  signals.push(isNum(s.sharesOutstanding) && isNum(s.priorSharesOutstanding) && s.sharesOutstanding <= s.priorSharesOutstanding); // 7 no dilution
  signals.push(isNum(grossMarginNow) && isNum(prior.grossMargin) && grossMarginNow > prior.grossMargin); // 8 margin up
  signals.push(isNum(turnoverNow) && isNum(prior.assetTurnover) && turnoverNow > prior.assetTurnover);   // 9 turnover up

  const score = signals.filter(Boolean).length;
  return { score, signals, of: 9 };
}

/** Rule of 40: growth plus margin should exceed 40 for a healthy software business. */
export function ruleOf40(s) {
  const growth = isNum(s.priorRevenue) && s.priorRevenue !== 0 ? (s.revenue - s.priorRevenue) / s.priorRevenue : null;
  const margin = fcfMargin(s);
  if (!isNum(growth) || !isNum(margin)) return null;
  return { growth, margin, total: growth + margin, passes: growth + margin >= 0.4 };
}

// ---------------------------------------------------------------- diagnostics

/**
 * Red flags a careful analyst would want to see first.
 *
 * Not a verdict: each flag is a question to ask, with the number that raised it.
 */
export function qualityFlags(s) {
  const flags = [];
  const push = (flag, severity, detail) => flags.push({ flag, severity, detail });

  const accrualGap = sub(s.operatingCashFlow, s.netIncome);
  if (isNum(accrualGap) && isNum(s.netIncome) && s.netIncome > 0 && accrualGap < 0) {
    push('profit-not-backing-with-cash', 'high', 'net income is positive but operating cash flow is lower');
  }
  const fcf = freeCashFlow(s);
  if (isNum(fcf) && fcf < 0) {
    push('negative-free-cash-flow', 'high', `free cash flow is ${Math.round(fcf)}`);
  }
  const dte = debtToEquity(s);
  if (isNum(dte) && dte > 2) {
    push('high-leverage', 'high', `debt is ${dte.toFixed(2)}x equity`);
  }
  const cr = currentRatio(s);
  if (isNum(cr) && cr < 1) {
    push('liquidity-below-one', 'high', `current ratio is ${cr.toFixed(2)}`);
  }
  const gm = grossMargin(s);
  const gmPrior = div(s.priorGrossProfit, s.priorRevenue);
  if (isNum(gm) && isNum(gmPrior) && gm < gmPrior - 0.02) {
    push('margin-compression', 'medium', `gross margin fell from ${(gmPrior * 100).toFixed(1)}% to ${(gm * 100).toFixed(1)}%`);
  }
  if (isNum(s.sharesOutstanding) && isNum(s.priorSharesOutstanding) && s.sharesOutstanding > s.priorSharesOutstanding * 1.02) {
    push('dilution', 'medium', 'share count rose by more than 2%');
  }
  const nm = netMargin(s);
  if (isNum(nm) && nm < 0) {
    push('loss-making', 'medium', `net margin is ${(nm * 100).toFixed(1)}%`);
  }
  const cover = interestCoverage(s);
  if (isNum(cover) && cover < 2) {
    push('thin-interest-cover', 'medium', `interest is covered ${cover.toFixed(1)}x`);
  }
  return flags;
}

/** Every ratio at once, with the inputs that produced them. */
export function analyse(s) {
  return {
    profitability: {
      grossMargin: grossMargin(s),
      operatingMargin: operatingMargin(s),
      netMargin: netMargin(s),
      ebitdaMargin: ebitdaMargin(s),
      roe: roe(s),
      roa: roa(s),
      roic: roic(s),
    },
    efficiency: {
      assetTurnover: assetTurnover(s),
      inventoryTurnover: inventoryTurnover(s),
      daysInventory: daysInventory(s),
      receivablesTurnover: receivablesTurnover(s),
      daysSalesOutstanding: daysSalesOutstanding(s),
    },
    liquidity: {
      currentRatio: currentRatio(s),
      quickRatio: quickRatio(s),
      cashRatio: cashRatio(s),
      workingCapital: workingCapital(s),
    },
    leverage: {
      debtToEquity: debtToEquity(s),
      debtToAssets: debtToAssets(s),
      netDebt: netDebt(s),
      interestCoverage: interestCoverage(s),
    },
    perShare: {
      eps: epsBasic(s),
      bookValuePerShare: bookValuePerShare(s),
      peRatio: peRatio(s),
      pbRatio: pbRatio(s),
      psRatio: psRatio(s),
      dividendPayout: dividendPayout(s),
    },
    valuation: {
      marketCap: marketCap(s),
      enterpriseValue: enterpriseValue(s),
      evEbitda: evEbitda(s),
      freeCashFlow: freeCashFlow(s),
      fcfMargin: fcfMargin(s),
      grahamNumber: grahamNumber(s),
    },
    composites: {
      dupont: dupont(s),
      altmanZ: altmanZ(s),
      piotroskiF: piotroskiF(s),
      ruleOf40: ruleOf40(s),
    },
    flags: qualityFlags(s),
  };
}
