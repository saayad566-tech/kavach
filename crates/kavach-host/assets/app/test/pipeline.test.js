// The data pipeline: parse -> normalise -> infer -> coerce -> dedupe -> validate.
//
// Each stage is tested on the inputs that actually break CSV pipelines in the wild: quoted
// commas, escaped quotes, CRLF line endings, a BOM, ragged rows and blank cells.

import test from 'node:test';
import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

import {
  parseCsv, normaliseHeader, toRecords, inferSchema, coerce, validate,
  dedupe, mergeOn, readDataset, runPipeline,
} from '../src/pipeline.js';

const here = dirname(fileURLToPath(import.meta.url));

test('parseCsv reads a plain table', () => {
  const { header, rows } = parseCsv('a,b\n1,2\n3,4\n');
  assert.deepEqual(header, ['a', 'b']);
  assert.deepEqual(rows, [['1', '2'], ['3', '4']]);
});

test('parseCsv handles quoted commas and escaped quotes', () => {
  const { rows } = parseCsv('name,note\n"Smith, J","said ""hi"""\n');
  assert.deepEqual(rows[0], ['Smith, J', 'said "hi"']);
});

test('parseCsv survives CRLF, a BOM and blank lines', () => {
  const { header, rows } = parseCsv('\uFEFFa,b\r\n1,2\r\n\r\n3,4\r\n');
  assert.deepEqual(header, ['a', 'b']);
  assert.deepEqual(rows, [['1', '2'], ['3', '4']]);
});

test('parseCsv on empty input returns an empty table', () => {
  assert.deepEqual(parseCsv(''), { header: [], rows: [] });
});

test('normaliseHeader produces predictable field names', () => {
  assert.equal(normaliseHeader('Close Price'), 'close_price');
  assert.equal(normaliseHeader('  Symbol '), 'symbol');
  assert.equal(normaliseHeader('2026'), 'c_2026', 'a leading digit is prefixed');
  assert.equal(normaliseHeader('!!!'), 'column', 'empty results still get a name');
});

test('toRecords pads ragged rows', () => {
  const records = toRecords(parseCsv('a,b,c\n1,2\n'));
  assert.deepEqual(records[0], { a: '1', b: '2', c: '' });
});

test('inferSchema types columns and counts gaps', () => {
  const records = [
    { close: '10.5', date: '2026-01-05', name: 'AAA' },
    { close: '', date: '2026-01-06', name: 'BBB' },
  ];
  const schema = inferSchema(records);
  assert.equal(schema.close.type, 'number');
  assert.equal(schema.close.missing, 1);
  assert.equal(schema.date.type, 'date');
  assert.equal(schema.name.type, 'string');
  assert.equal(schema.name.distinct, 2);
});

test('a mostly-numeric column stays numeric and the stragglers become null', () => {
  // One bad cell must not poison the column: the majority decides, and validation reports
  // the value that did not parse.
  const schema = inferSchema([{ v: '1' }, { v: '2' }, { v: '3' }, { v: 'oops' }]);
  assert.equal(schema.v.type, 'number');
});

test('a column with no clear majority falls back to string', () => {
  assert.equal(inferSchema([{ v: '1' }, { v: 'abc' }]).v.type, 'string', 'a tie is text');
  assert.equal(inferSchema([{ v: 'abc' }, { v: 'def' }]).v.type, 'string');
});

test('coerce converts values and nulls out the unparseable', () => {
  const records = [{ n: '12', d: '2026-01-05', s: ' x ', blank: '' }, { n: 'oops', d: 'nope', s: 'y', blank: '' }];
  // An explicit schema isolates coercion from type inference.
  const schema = { n: { type: 'number' }, d: { type: 'date' }, s: { type: 'string' }, blank: { type: 'string' } };
  const typed = coerce(records, schema);
  assert.equal(typed[0].n, 12);
  assert.equal(typed[0].s, 'x');
  assert.equal(typed[0].d, '2026-01-05T00:00:00.000Z');
  assert.equal(typed[0].blank, null, 'an empty cell is null, not an empty string');
  assert.equal(typed[1].n, null, 'an unparseable number is null, not NaN');
  assert.equal(typed[1].d, null);
});

test('validate reports required, type and range errors with row numbers', () => {
  const records = [{ amount: 5 }, { amount: null }, { amount: 'x' }, { amount: 500 }];
  const result = validate(records, {
    amount: { required: true, type: 'number', min: 0, max: 100 },
  });
  assert.equal(result.ok, false);
  assert.deepEqual(result.errors.map((e) => e.row), [1, 2, 3]);
  assert.match(result.errors[0].message, /required/);
  assert.match(result.errors[1].message, /number/);
  assert.match(result.errors[2].message, /at most/);
});

test('validate passes clean data', () => {
  const result = validate([{ amount: 5 }], { amount: { required: true, type: 'number', min: 0 } });
  assert.deepEqual(result, { ok: true, errors: [] });
});

test('dedupe keeps the first occurrence and counts the rest', () => {
  const records = [{ id: 1 }, { id: 2 }, { id: 1 }];
  const { unique, duplicates } = dedupe(records, ['id']);
  assert.equal(unique.length, 2);
  assert.equal(duplicates.length, 1);
});

test('mergeOn performs an inner join', () => {
  const left = [{ id: 1, name: 'a' }, { id: 2, name: 'b' }];
  const right = [{ id: 1, score: 9 }];
  const merged = mergeOn(left, right, 'id');
  assert.equal(merged.length, 1);
  assert.deepEqual(merged[0], { id: 1, name: 'a', score: 9 });
});

test('runPipeline runs every stage end to end', () => {
  const csv = 'symbol,close\nAAA,10\nAAA,10\nBBB,notanumber\n';
  const result = runPipeline(csv, { close: { required: true, type: 'number' } });
  assert.deepEqual(result.stages, ['parse', 'normalise', 'infer', 'coerce', 'dedupe', 'validate']);
  assert.equal(result.records.length, 2, 'the duplicate row is removed');
  assert.equal(result.duplicates, 1);
  assert.equal(result.validation.ok, false, 'the bad number is reported');
  assert.equal(result.records[0].close, 10);
  assert.equal(result.records[1].close, null);
});

test('readDataset loads the bundled sample file', async () => {
  const dataset = await readDataset(join(here, '..', 'data', 'sample.csv'));
  assert.ok(dataset.rows >= 15, `expected the sample rows, got ${dataset.rows}`);
  assert.equal(dataset.schema.close.type, 'number');
  assert.equal(dataset.schema.symbol.type, 'string');
  assert.equal(typeof dataset.records[0].close, 'number');
  assert.equal(dataset.records[0].symbol, 'AAA');
});

test('the sample dataset supports the analysis functions', async () => {
  const { records } = await readDataset(join(here, '..', 'data', 'sample.csv'));
  const { groupAggregate, meanField, correlation, timeBucket } = await import('../src/analytics.js');

  const bySymbol = groupAggregate(records, 'symbol', 'volume', 'sum');
  assert.equal(bySymbol.length, 3);
  assert.ok(meanField(records, 'close') > 0);
  assert.ok(correlation(records, 'close', 'volume') !== undefined);
  assert.equal(timeBucket(records, 'date', 'month')[0].bucket, '2026-01');
});
