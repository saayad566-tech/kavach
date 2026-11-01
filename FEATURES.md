# What a generated application can do

Every Kavach-generated app ships with a data pipeline and an analysis library, and every
function below has a test. The counts are of things a user can actually call, not of lines of
code.

The pipeline is domain-agnostic: a "record" is any object, so the same stages work whether the
data is invoices, orders, patients or stock prices. Nothing here contacts a network — the
analysis runs in the app's own process, on this machine.

---

## Data pipeline — 10 functions (`src/pipeline.js`)

| # | Function | What it does |
|---|---|---|
| 1 | `parseCsv(text)` | CSV to header + rows; handles quoted commas, escaped quotes, CRLF, a UTF-8 BOM and blank lines |
| 2 | `normaliseHeader(name)` | `Close Price` → `close_price`; ASCII, snake_case, digit-prefix safe |
| 3 | `toRecords(table)` | Rows to objects keyed by header; ragged rows are padded, not dropped |
| 4 | `inferSchema(records)` | Per-field type by majority, with missing counts and cardinality |
| 5 | `coerce(records, schema)` | Numbers and ISO dates; unparseable cells become `null`, never `NaN` |
| 6 | `validate(records, rules)` | Required / type / min / max, with the row number of every failure |
| 7 | `dedupe(records, keys)` | Removes duplicates by key fields and reports what was dropped |
| 8 | `mergeOn(left, right, key)` | Inner join two datasets |
| 9 | `readDataset(path)` | Reads one local CSV or JSON file and runs the whole pipeline |
| 10 | `runPipeline(text, rules)` | Every stage in one call, returning the stage list it ran |

The type-inference rule is worth stating: a column becomes numeric when **most** of its values
parse as numbers, not when all of them do. Widening on the first bad cell looks safe and is
not — one typo in a thousand rows would make every numeric function return `null`. The
stragglers become `null` and validation reports them.

## Analysis — 28 functions (`src/analytics.js`)

**Descriptive (9)** — `count`, `sumField`, `meanField`, `medianField`, `modeField`,
`minField`, `maxField`, `varianceField`, `stddevField`

**Distribution (4)** — `percentileField` (nearest-rank), `frequency`, `distinctCount`,
`outliersIqr` (Tukey's fences)

**Grouping and ranking (3)** — `groupAggregate` (sum/mean/min/max/count),
`topN` (either direction), `sortBy` (numeric or lexical, stable)

**Time series (4)** — `growthRate` (period over period, zero-safe), `cagr`,
`movingAverage` (gap-aware), `timeBucket` (day / week / month)

**Statistics (3)** — `linearTrend` (slope, intercept, r², direction), `zScores`,
`correlation` (Pearson, `null` when either side has no variance)

**Querying (2)** — `filterWhere` (eq/ne/gt/gte/lt/lte/contains/in),
`paginate` (page, size, total, page count)

**Reporting (3)** — `describeAll` (every numeric field at once), `toCsv` (RFC-style quoting),
`dataQuality` (missing, wrong type, duplicate rows)

Every function returns `null` rather than `NaN` when the answer is undefined, so a caller can
distinguish "no data" from "the answer is zero". That distinction is tested.

## Fundamental analysis — 39 functions (`src/fundamentals.js`)

For applications that analyse a business rather than a generic table. Every function takes one
period of statements and returns a number, or `null` when the inputs cannot support it — never
`NaN`, never `Infinity`.

**Profitability (8)** — gross, operating, net and EBITDA margin; ROE, ROA, ROIC (NOPAT over
equity plus debt), effective tax rate

**Efficiency (5)** — asset turnover, inventory turnover, days inventory, receivables turnover,
days sales outstanding

**Liquidity (4)** — current, quick and cash ratio; working capital

**Leverage (4)** — debt/equity, debt/assets, net debt, interest cover

**Per share (6)** — EPS, book value per share, P/E, P/B, P/S, dividend payout

**Valuation (6)** — market cap, enterprise value, EV/EBITDA, free cash flow, FCF margin,
Graham number (undefined for loss-makers, deliberately)

**Composites (4)** — three-factor DuPont (which multiplies back to ROE), Altman Z-score with
safe/grey/distress zones, Piotroski F-score (nine signals across two periods), Rule of 40

**Diagnostics (2)** — `qualityFlags` (accrual gap, negative FCF, high leverage, liquidity below
one, margin compression, dilution, thin interest cover) and `analyse`, which returns every
group at once

`normaliseStatement` maps any reasonable spelling onto the canonical field names — the pipeline
lowercases CSV headers, so `totalEquity` arrives as `totalequity` and would otherwise read as
missing. Numeric strings and `1,000`-style values are converted; unknown columns are dropped
rather than guessed at.

### What this is not

It is not a data feed. Kavach computes these ratios from `data/statements.csv`, which you
supply — from an annual report, an exchange filing, or a subscription you already pay for.
Nothing is fetched, so there is no quota, no account, and no data leaving the machine. An app
that quietly scraped exchanges would be both legally fraught and a different product.

## HTTP API — 10 routes (`src/server.js`)

| Route | Returns |
|---|---|
| `GET /api/health` | Liveness, collections, row count, `analysis: true` |
| `GET /api/analysis/overview` | Source, rows, schema, numeric summary, quality report |
| `GET /api/analysis/top?field=&n=` | Top N records by a field |
| `GET /api/analysis/group?key=&field=&how=` | Grouped aggregation |
| `GET /api/analysis/trend?field=` | Least-squares trend over a field |
| `GET /api/analysis/buckets?date=&bucket=` | Day / week / month buckets |
| `GET /api/analysis/filter?field=&op=&value=` | Filtered records |
| `GET /api/analysis/rows?field=&dir=&page=&size=` | Sorted, paginated records |
| `GET /api/analysis/export.csv` | The whole dataset as CSV |

## UI

The generated page renders an analysis panel above the records table. It names no fields of
its own: it reads the inferred schema, picks the first numeric field and the first
low-cardinality text field, and labels the tables from those — "Summary of amount", "Largest
by amount", "amount by category" for the bundled placeholder dataset, and the equivalent for
whatever dataset you drop in. If there is no numeric field it says so rather than showing an
empty table.

The panel also links to the dataset as CSV (`/api/analysis/export.csv`) and appends a trend
line for the numeric field it found — "amount is rising (slope 1.561, r² 0.00)".

The bundled `data/sample.csv` is a **domain-neutral placeholder** (`name, category, amount,
quantity, date`), not a fixture for one kind of application. Replace it with your own CSV and
the panel adapts.

## CRUD (unchanged)

The original store, validation and record endpoints still work: create, list, delete, with
`400` and a field-level message on invalid input.

---

## How this is verified

```
crates/kavach-host/assets/app/test/analytics.test.js   25 tests, one per feature group
crates/kavach-host/assets/app/test/pipeline.test.js    17 tests, one per stage
generated project, node --test                         50 tests
```

The test files are not written by the generator — they are real files embedded into every
generated project, so a generated app ships with its own proof that its analysis works. The
end-to-end run in this repository generates a project, starts its server, and asserts that
`/api/analysis/*` returns correct numbers for the bundled sample dataset.
