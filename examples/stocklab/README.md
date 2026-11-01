# StockLab — an example application

**This is not the product.** Kavach is an app *builder*: it turns a description into a working
project. StockLab is one such project, kept in the repository as a worked example of what the
builder produces — a domain with real arithmetic, a data pipeline, a test suite and an API.

It is here because a builder is only as good as the applications it can generate, and an
example you can run is more convincing than a claim. Nothing about Kavach depends on it.

---

Fundamental analysis of listed companies, computed on your machine from statements you supply.

```bash
cd stocklab
npm start          # http://127.0.0.1:4270
npm test           # 20 tests
```

No dependencies, no account, no data feed. Node 20 or newer.

## What it does

- **Screening** — filter the universe on any metric (`roe > 0.25`, `debtToEquity < 1.5`), with
  companies that lack the metric reported as *excluded* rather than silently failing.
- **Ranking** — order the universe by any ratio.
- **Sector analysis** — median per sector, with each company's percentile *inside its own
  sector*, because a 20% ROE means different things in software and in steel.
- **Peer scoring** — each company scored against its sector across several metrics, with
  lower-is-better metrics inverted.
- **Per-company report** — 39 ratios and composites: margins, ROE/ROA/ROIC, turnover, liquidity,
  leverage, per-share and valuation multiples, DuPont, Altman Z, Piotroski F, Graham number,
  Rule of 40, and quality flags.
- **DCF valuation** — free cash flow model with a sensitivity grid across discount and growth,
  a margin of safety against the market price, and the terminal value's share of the total
  (which tells you how much of the answer is assumption).

## Data

`data/statements.csv` — one row per company per period. Columns are ordinary accounting
fields; `company`, `name`, `sector` and `period` identify the row, and every other column is
read as a number.

Point it at another file with `STOCKLAB_DATA=/path/to/statements.csv npm start`.

Two periods per company are needed for the measures that compare time: Piotroski F, margin
trend, growth. With one period those return null rather than a guess.

Where to get the data: the company's own annual report, an exchange filing (SEC EDGAR, NSE/BSE,
Companies House), or a data subscription you already pay for. This application does not fetch
anything.

## What it is not

Not a trading terminal and not a data vendor. There is no live feed, no scraping, and no
portfolio execution. Every number here is computed from a file on disk, so it is reproducible,
inspectable and works offline — which is also why it cannot tell you what the price is right
now. Pair it with whatever market data you already have.

Not investment advice. The ratios are standard; the assumptions in the DCF are yours.
