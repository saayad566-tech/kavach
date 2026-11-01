// The data pipeline for a generated Kavach application.
//
// Shape: source -> parse -> normalise -> coerce -> validate -> records -> analysis.
//
// Every stage is a pure function except `readDataset`, which reads one local file. Nothing
// here opens a socket: a Kavach app does its analysis on this machine, which is the whole
// point of the product.
//
// The pipeline is deliberately generic. It knows nothing about invoices, orders or stocks —
// a "record" is any object, so the same stages work for any application Kavach generates.

import { readFile } from 'node:fs/promises';
import { isNum } from './types.js';

/** Split one CSV line, honouring double quotes and escaped quotes. */
function splitCsvLine(line) {
  const cells = [];
  let current = '';
  let quoted = false;
  for (let i = 0; i < line.length; i += 1) {
    const char = line[i];
    if (quoted) {
      if (char === '"' && line[i + 1] === '"') {
        current += '"';
        i += 1;
      } else if (char === '"') {
        quoted = false;
      } else {
        current += char;
      }
    } else if (char === '"') {
      quoted = true;
    } else if (char === ',') {
      cells.push(current);
      current = '';
    } else {
      current += char;
    }
  }
  cells.push(current);
  return cells;
}

/** Parse CSV text into a header and rows. Handles CRLF, a UTF-8 BOM and blank lines. */
export function parseCsv(text) {
  const clean = String(text).replace(/^\uFEFF/, '').replace(/\r\n?/g, '\n');
  const lines = clean.split('\n').filter((line) => line.trim() !== '');
  if (!lines.length) return { header: [], rows: [] };
  const header = splitCsvLine(lines[0]).map(normaliseHeader);
  const rows = lines.slice(1).map((line) => splitCsvLine(line));
  return { header, rows };
}

/** snake_case, ASCII, no leading digits. Keeps field names predictable across datasets. */
export function normaliseHeader(name) {
  const cleaned = String(name)
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '_')
    .replace(/^_+|_+$/g, '');
  const safe = cleaned || 'column';
  return /^[0-9]/.test(safe) ? `c_${safe}` : safe;
}

/** Turn a parsed table into records keyed by the header. Short rows are padded. */
export function toRecords(table) {
  const { header, rows } = table;
  return rows.map((row) => {
    const record = {};
    header.forEach((key, index) => {
      record[key] = row[index] === undefined ? '' : row[index];
    });
    return record;
  });
}

/** Guess a type per field from the values, and report what is missing. */
export function inferSchema(records) {
  const schema = {};
  for (const record of records) {
    for (const [field, raw] of Object.entries(record)) {
      const value = raw === null || raw === undefined ? '' : String(raw).trim();
      const entry = schema[field] || (schema[field] = {
        kinds: { number: 0, date: 0, string: 0 },
        missing: 0,
        distinct: new Set(),
      });
      if (value === '') {
        entry.missing += 1;
        continue;
      }
      entry.distinct.add(value);
      const kind = looksLikeNumber(value) ? 'number' : looksLikeDate(value) ? 'date' : 'string';
      entry.kinds[kind] += 1;
    }
  }
  const out = {};
  for (const [field, entry] of Object.entries(schema)) {
    const { number: nums, date: dates, string: strings } = entry.kinds;
    const total = nums + dates + strings;
    // Majority wins, ties go to string.
    //
    // Widening a column to string the moment one cell fails to parse looks safe and is not:
    // a single typo in a thousand rows would make every numeric function return null. The
    // stragglers become null at the coerce stage and are reported by validation instead.
    let type = 'string';
    if (total > 0 && nums * 2 > total) type = 'number';
    else if (total > 0 && dates * 2 > total) type = 'date';

    out[field] = { type, missing: entry.missing, distinct: entry.distinct.size };
  }
  return out;
}

function looksLikeNumber(value) {
  return /^-?\d+(\.\d+)?([eE][-+]?\d+)?$/.test(value);
}

function looksLikeDate(value) {
  return /^\d{4}-\d{2}-\d{2}([T ].*)?$/.test(value);
}

/** Apply a schema, converting values in place-free fashion. Bad values become null. */
export function coerce(records, schema) {
  return records.map((record) => {
    const out = {};
    for (const [field, raw] of Object.entries(record)) {
      const meta = schema[field];
      const text = raw === null || raw === undefined ? '' : String(raw).trim();
      if (text === '') {
        out[field] = null;
      } else if (meta && meta.type === 'number') {
        const value = Number(text);
        out[field] = Number.isFinite(value) ? value : null;
      } else if (meta && meta.type === 'date') {
        const value = new Date(text);
        out[field] = Number.isNaN(value.getTime()) ? null : value.toISOString();
      } else {
        out[field] = text;
      }
    }
    return out;
  });
}

/**
 * Validate records against explicit rules.
 *
 * Rules look like { field: { required: true, type: 'number', min: 0 } }. Errors carry the
 * row index so the UI can point at the offending line rather than shrugging.
 */
export function validate(records, rules) {
  const errors = [];
  records.forEach((record, index) => {
    for (const [field, rule] of Object.entries(rules || {})) {
      const value = record[field];
      const missing = value === null || value === undefined || value === '';
      if (rule.required && missing) {
        errors.push({ row: index, field, message: 'is required' });
        continue;
      }
      if (missing) continue;
      if (rule.type === 'number' && !isNum(value)) {
        errors.push({ row: index, field, message: 'must be a number' });
      }
      if (rule.type === 'string' && typeof value !== 'string') {
        errors.push({ row: index, field, message: 'must be text' });
      }
      if (rule.min !== undefined && isNum(value) && value < rule.min) {
        errors.push({ row: index, field, message: `must be at least ${rule.min}` });
      }
      if (rule.max !== undefined && isNum(value) && value > rule.max) {
        errors.push({ row: index, field, message: `must be at most ${rule.max}` });
      }
    }
  });
  return { ok: errors.length === 0, errors };
}

/** Remove duplicate records by key fields, reporting what was dropped. */
export function dedupe(records, keys) {
  const seen = new Set();
  const unique = [];
  const duplicates = [];
  for (const record of records) {
    const key = JSON.stringify(keys && keys.length ? keys.map((k) => record[k]) : record);
    if (seen.has(key)) duplicates.push(record);
    else {
      seen.add(key);
      unique.push(record);
    }
  }
  return { unique, duplicates };
}

/** Inner join on one key field. */
export function mergeOn(left, right, key) {
  const index = new Map();
  for (const record of right) index.set(record[key], record);
  const out = [];
  for (const record of left) {
    const match = index.get(record[key]);
    if (match) out.push({ ...record, ...match });
  }
  return out;
}

/** Read one local file and run it through the pipeline. */
export async function readDataset(path) {
  const text = await readFile(path, 'utf8');
  if (path.toLowerCase().endsWith('.json')) {
    const parsed = JSON.parse(text);
    const records = Array.isArray(parsed) ? parsed : parsed.records || [];
    const schema = inferSchema(records);
    return { records: coerce(records, schema), schema, source: path, rows: records.length };
  }
  const table = parseCsv(text);
  const raw = toRecords(table);
  const schema = inferSchema(raw);
  return { records: coerce(raw, schema), schema, source: path, rows: raw.length };
}

/** One call that runs the whole pipeline over a CSV string. Used by the server and tests. */
export function runPipeline(text, rules = {}) {
  const table = parseCsv(text);
  const raw = toRecords(table);
  const schema = inferSchema(raw);
  const typed = coerce(raw, schema);
  const { unique, duplicates } = dedupe(typed, table.header);
  const validation = validate(unique, rules);
  return {
    header: table.header,
    schema,
    records: unique,
    duplicates: duplicates.length,
    validation,
    stages: ['parse', 'normalise', 'infer', 'coerce', 'dedupe', 'validate'],
  };
}
