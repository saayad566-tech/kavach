// Discounted cash flow valuation, with the sensitivity table that makes it honest.
//
// A single DCF number is a false precision: change the discount rate by a point and the "fair
// value" moves by a third. This module returns the number *and* the grid around it, because the
// range is the finding and the point estimate is the decoration.

const isNum = (v) => typeof v === 'number' && Number.isFinite(v);

/**
 * Value a business from its free cash flow.
 *
 * inputs:
 *   freeCashFlow   current FCF, in whole currency units
 *   growth         expected FCF growth per year, e.g. 0.08
 *   discount       required return / WACC, e.g. 0.10
 *   terminalGrowth perpetuity growth after the forecast, e.g. 0.025
 *   years          forecast horizon, default 10
 *   shares         shares outstanding, for a per-share value
 *   netDebt        debt minus cash; subtracted from enterprise value
 *
 * Returns null when the inputs cannot produce a meaningful answer — notably when the terminal
 * growth rate is not below the discount rate, which makes the perpetuity infinite.
 */
export function dcf(inputs) {
  const {
    freeCashFlow, growth, discount, terminalGrowth, shares, netDebt,
  } = inputs;
  const years = Number.isInteger(inputs.years) ? inputs.years : 10;

  if (!isNum(freeCashFlow) || !isNum(growth) || !isNum(discount) || !isNum(terminalGrowth)) return null;
  if (years < 1 || years > 50) return null;
  if (discount <= terminalGrowth) {
    return { error: 'terminal growth must be below the discount rate, or the value is infinite' };
  }
  if (discount <= -1 || growth <= -1) return null;

  const forecast = [];
  let presentValue = 0;
  let cash = freeCashFlow;
  for (let year = 1; year <= years; year += 1) {
    cash = cash * (1 + growth);
    const discounted = cash / (1 + discount) ** year;
    presentValue += discounted;
    forecast.push({ year, cashFlow: cash, presentValue: discounted });
  }

  const terminalValue = (cash * (1 + terminalGrowth)) / (discount - terminalGrowth);
  const terminalPresent = terminalValue / (1 + discount) ** years;
  const enterpriseValue = presentValue + terminalPresent;
  const equityValue = isNum(netDebt) ? enterpriseValue - netDebt : enterpriseValue;
  const perShare = isNum(shares) && shares > 0 ? equityValue / shares : null;

  return {
    forecast,
    presentValueOfForecast: presentValue,
    terminalValue,
    presentValueOfTerminal: terminalPresent,
    terminalShareOfValue: enterpriseValue === 0 ? null : terminalPresent / enterpriseValue,
    enterpriseValue,
    equityValue,
    perShare,
  };
}

/**
 * Sensitivity grid: the same DCF across a range of discount and growth assumptions.
 *
 * Returns a matrix of per-share values so the reader can see how narrow the "right" answer is.
 */
export function sensitivity(inputs, options = {}) {
  const discounts = options.discounts || [-0.02, -0.01, 0, 0.01, 0.02].map((d) => inputs.discount + d);
  const growths = options.growths || [-0.02, -0.01, 0, 0.01, 0.02].map((g) => inputs.growth + g);

  const rows = discounts.map((discount) => ({
    discount,
    cells: growths.map((growth) => {
      const result = dcf({ ...inputs, discount, growth });
      return result && !result.error ? result.perShare : null;
    }),
  }));

  const values = rows.flatMap((row) => row.cells).filter(isNum);
  return {
    discounts,
    growths,
    rows,
    min: values.length ? Math.min(...values) : null,
    max: values.length ? Math.max(...values) : null,
    spread: values.length ? Math.max(...values) - Math.min(...values) : null,
  };
}

/** Margin of safety against a market price: how far the price sits below the estimated value. */
export function marginOfSafety(estimatedValue, price) {
  if (!isNum(estimatedValue) || !isNum(price) || estimatedValue <= 0) return null;
  return (estimatedValue - price) / estimatedValue;
}

/** Implied expectation: what growth the current price requires, found by bisection. */
export function impliedGrowth(inputs, price, options = {}) {
  if (!isNum(price) || price <= 0 || !isNum(inputs.shares) || inputs.shares <= 0) return null;
  let low = -0.5;
  let high = 1.0;
  const valueAt = (growth) => {
    const result = dcf({ ...inputs, growth });
    return result && !result.error ? result.perShare : null;
  };
  const lowValue = valueAt(low);
  const highValue = valueAt(high);
  if (!isNum(lowValue) || !isNum(highValue)) return null;
  if (price < lowValue || price > highValue) return null; // outside what this model can explain

  // Bisection: 60 iterations is well below double precision, and the function is monotonic.
  for (let i = 0; i < 60; i += 1) {
    const mid = (low + high) / 2;
    const value = valueAt(mid);
    if (!isNum(value)) return null;
    if (value < price) low = mid;
    else high = mid;
  }
  return (low + high) / 2;
}
