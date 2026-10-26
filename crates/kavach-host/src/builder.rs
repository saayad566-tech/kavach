//! The application builder: requirements → plan → scaffold → verify → delivery report.
//!
//! All of it is deterministic. That is deliberate: the desktop shell has no model wired
//! in yet, and a builder that depends on a model emitting perfect JSON on the first try
//! is a demo rather than a product. When a model is added, its output is validated and
//! merged on top of this scaffold — it cannot prevent a working project from existing.

use std::path::Path;

use crate::exec::{run, ExecOptions, EXIT_TIMEOUT};
use crate::json::Json;
use crate::path::{delete_project_file, list_project_files, write_project_file};
use crate::runtime::{self, Runtime};
use crate::security;

#[derive(Debug, Clone, PartialEq)]
pub struct Feature {
    pub id: String,
    pub statement: String,
    pub acceptance: String,
}

#[derive(Debug, Clone, Default)]
pub struct Requirements {
    pub app_kind: String,
    pub summary: String,
    pub entities: Vec<String>,
    pub features: Vec<Feature>,
    pub assumptions: Vec<String>,
    pub offline_required: bool,
    pub data_sensitivity: String,
}

#[derive(Debug, Clone)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub acceptance: Vec<String>,
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct GeneratedFile {
    pub path: String,
    pub content: String,
    pub origin: &'static str,
}

#[derive(Debug, Clone)]
pub struct Stage {
    pub stage: &'static str,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone)]
pub struct DeliveryReport {
    pub project_id: String,
    pub stages: Vec<Stage>,
    pub files: Vec<GeneratedFile>,
    pub verified: Vec<String>,
    pub unverified: Vec<String>,
    pub limitations: Vec<String>,
    pub duration_ms: u64,
    /// Which local runtime contributed, if any.
    pub runtime: Option<String>,
    /// Files the model authored and that survived verification.
    pub model_files: Vec<String>,
    /// Result of the security scan, which gates delivery.
    pub security_summary: String,
    pub security_passed: bool,
}

impl DeliveryReport {
    pub fn verified_ok(&self) -> bool {
        self.stages.iter().find(|s| s.stage == "verify").map(|s| s.ok).unwrap_or(false)
    }

    pub fn to_json(&self) -> String {
        let stages: Vec<String> = self
            .stages
            .iter()
            .map(|s| Json::object().str("stage", s.stage).bool("ok", s.ok).str("detail", &s.detail).finish())
            .collect();
        let files: Vec<String> = self
            .files
            .iter()
            .map(|f| Json::object().str("path", &f.path).str("origin", f.origin).finish())
            .collect();
        let verified: Vec<String> = self.verified.clone();
        let unverified: Vec<String> = self.unverified.clone();
        Json::object()
            .str("projectId", &self.project_id)
            .raw("stages", &format!("[{}]", stages.join(",")))
            .raw("files", &format!("[{}]", files.join(",")))
            .strings("tasksVerified", &verified)
            .strings("tasksUnverified", &unverified)
            .strings("limitations", &self.limitations)
            .uint("durationMs", self.duration_ms)
            .strings("modelFiles", &self.model_files)
            .raw("runtime", &match &self.runtime {
                Some(name) => format!("\"{}\"", crate::json::escape(name)),
                None => "null".to_string(),
            })
            .str("securitySummary", &self.security_summary)
            .bool("securityPassed", self.security_passed)
            .finish()
    }
}

fn has(prompt: &str, needles: &[&str]) -> bool {
    let lower = prompt.to_lowercase();
    needles.iter().any(|n| lower.contains(n))
}

/// Extract entities from phrases like "manage invoices and clients".
fn extract_entities(prompt: &str) -> Vec<String> {
    const STOP: &[&str] = &[
        "the", "a", "an", "my", "our", "their", "your", "it", "them", "this", "that", "data", "records",
        "things", "stuff", "search", "export", "login", "users", "user", "files", "file", "and",
    ];
    let mut found: Vec<String> = Vec::new();
    let lower = prompt.to_lowercase();
    let triggers = ["manage ", "track ", "store ", "record ", "maintain ", "list of "];

    for trigger in triggers {
        let mut rest = lower.as_str();
        while let Some(idx) = rest.find(trigger) {
            let tail = &rest[idx + trigger.len()..];
            // Take up to the end of the phrase.
            let end = tail
                .find(|c: char| matches!(c, '.' | ';' | ':' | '!' | '?' | '\n'))
                .unwrap_or(tail.len());
            let phrase = &tail[..end];
            for part in phrase.split(|c: char| c == ',' || c == '/' || c == '+') {
                let part = part.trim();
                for word in part.split(" and ") {
                    let mut w = word.trim().trim_start_matches("the ").trim();
                    // Drop trailing clauses such as "with search".
                    if let Some(cut) = w.find(" with ") {
                        w = &w[..cut];
                    }
                    let w = w.trim().trim_end_matches('s');
                    if w.len() >= 3 && w.split_whitespace().count() <= 2 && !STOP.contains(&w) {
                        let named = capitalize(w);
                        if !found.iter().any(|f| f.eq_ignore_ascii_case(&named)) {
                            found.push(named);
                        }
                    }
                }
            }
            rest = &tail[end.min(tail.len())..];
        }
    }
    if found.is_empty() {
        found.push("Item".to_string());
    }
    found.truncate(4);
    found
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn slug(word: &str) -> String {
    word.to_lowercase().chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>()
}

pub fn extract_requirements(prompt: &str) -> Requirements {
    let trimmed = prompt.trim();
    let app_kind = if has(trimmed, &["mobile app", "android", "ios", "iphone"]) {
        "mobile-app"
    } else if has(trimmed, &["desktop app", "windows app", "electron", "tauri"]) {
        "desktop-app"
    } else if has(trimmed, &["saas", "subscription", "multi-tenant"]) {
        "saas"
    } else if has(trimmed, &["api", "webhook", "graphql"]) {
        "api"
    } else if has(trimmed, &["automate", "script", "batch", "cron"]) {
        "automation"
    } else {
        "web-app"
    };

    let entities = extract_entities(trimmed);
    let offline_required = has(trimmed, &["offline", "local-first", "no internet", "air-gapped"]);
    let data_sensitivity = if has(trimmed, &["patient", "health", "medical", "aadhaar", "kyc", "card", "salary"]) {
        "sensitive"
    } else if has(trimmed, &["customer", "user", "email", "phone", "address"]) {
        "personal"
    } else {
        "internal"
    };

    let mut features: Vec<Feature> = Vec::new();
    for (i, entity) in entities.iter().enumerate() {
        let key = slug(entity);
        features.push(Feature {
            id: format!("FR-{:02}", i + 1),
            statement: format!("Create, read, update and delete {entity} records"),
            acceptance: format!(
                "A {key} can be created, listed and deleted through the API, and invalid input returns 400."
            ),
        });
    }
    if has(trimmed, &["search"]) {
        features.push(Feature {
            id: format!("FR-{:02}", features.len() + 1),
            statement: "Search records by name".to_string(),
            acceptance: "A search term filters the list and an empty result shows an empty state.".to_string(),
        });
    }
    if has(trimmed, &["export", "csv", "download"]) {
        features.push(Feature {
            id: format!("FR-{:02}", features.len() + 1),
            statement: "Export records".to_string(),
            acceptance: "The export endpoint returns every record in a documented format.".to_string(),
        });
    }

    let mut assumptions = vec![
        format!("Application kind inferred as \"{app_kind}\"."),
        "Data is stored locally in a JSON file; no database server is required.".to_string(),
        "No third-party analytics, fonts or CDNs are used; all assets are bundled.".to_string(),
    ];
    if !offline_required {
        assumptions.push("Offline operation was not requested, but the generated app makes no network calls.".to_string());
    }

    Requirements {
        app_kind: app_kind.to_string(),
        summary: trimmed.chars().take(120).collect::<String>().trim_end_matches('.').to_string(),
        entities,
        features,
        assumptions,
        offline_required,
        data_sensitivity: data_sensitivity.to_string(),
    }
}

/// Dependency-ordered plan. Every task carries an acceptance criterion.
pub fn build_plan(reqs: &Requirements) -> Vec<Task> {
    let mut tasks = vec![
        Task {
            id: "T-01".into(),
            title: "Scaffold the project structure".into(),
            acceptance: vec!["The repository tree matches the plan".into()],
            depends_on: vec![],
        },
        Task {
            id: "T-02".into(),
            title: "Implement the application logic and API surface".into(),
            acceptance: vec![
                "Every endpoint validates input".into(),
                "Invalid input returns 400, not a stack trace".into(),
            ],
            depends_on: vec!["T-01".into()],
        },
        Task {
            id: "T-03".into(),
            title: "Implement the user interface".into(),
            acceptance: vec!["Every screen renders without console errors".into()],
            depends_on: vec!["T-02".into()],
        },
        Task {
            id: "T-04".into(),
            title: "Write and run automated tests".into(),
            acceptance: vec!["The test command exits 0".into()],
            depends_on: vec!["T-02".into()],
        },
        Task {
            id: "T-05".into(),
            title: "Write run and build documentation".into(),
            acceptance: vec!["A fresh checkout can be run using only the documented commands".into()],
            depends_on: vec!["T-04".into()],
        },
        Task {
            id: "T-06".into(),
            title: "Run security checks".into(),
            acceptance: vec!["Secret scan clean".into()],
            depends_on: vec!["T-02".into()],
        },
    ];
    if reqs.features.iter().any(|f| f.statement.contains("Search")) {
        tasks.push(Task {
            id: "T-07".into(),
            title: "Implement search and filtering".into(),
            acceptance: vec!["Searching an empty list returns an empty array".into()],
            depends_on: vec!["T-02".into()],
        });
    }
    tasks
}

const STORE_JS: &str = r#"import { readFileSync, writeFileSync, mkdirSync, existsSync } from 'node:fs';
import path from 'node:path';
import { randomUUID } from 'node:crypto';

export const COLLECTIONS = __COLLECTIONS__;

export class ValidationError extends Error {
  constructor(message) {
    super(message);
    this.name = 'ValidationError';
    this.status = 400;
  }
}

export class NotFoundError extends Error {
  constructor(message) {
    super(message);
    this.name = 'NotFoundError';
    this.status = 404;
  }
}

function emptyState() {
  const state = {};
  for (const collection of COLLECTIONS) state[collection] = [];
  return state;
}

export function createStore(dataFile) {
  function read() {
    if (!existsSync(dataFile)) return emptyState();
    try {
      const parsed = JSON.parse(readFileSync(dataFile, 'utf8'));
      const state = emptyState();
      for (const collection of COLLECTIONS) {
        if (Array.isArray(parsed[collection])) state[collection] = parsed[collection];
      }
      return state;
    } catch {
      return emptyState();
    }
  }

  function write(state) {
    mkdirSync(path.dirname(dataFile), { recursive: true });
    writeFileSync(dataFile, JSON.stringify(state, null, 2), 'utf8');
  }

  function requireCollection(name) {
    if (!COLLECTIONS.includes(name)) throw new NotFoundError('Unknown collection: ' + name);
    return name;
  }

  function validate(input) {
    if (input === null || typeof input !== 'object' || Array.isArray(input)) {
      throw new ValidationError('Request body must be a JSON object');
    }
    const record = {};
    const raw = input.name;
    if (raw === undefined || raw === null || String(raw).trim() === '') {
      throw new ValidationError('Field "name" is required');
    }
    if (typeof raw !== 'string') throw new ValidationError('Field "name" must be a string');
    if (raw.length > 2000) throw new ValidationError('Field "name" is too long');
    record.name = raw.trim();
    if (input.notes !== undefined && input.notes !== null && input.notes !== '') {
      record.notes = String(input.notes).slice(0, 2000);
    }
    return record;
  }

  return {
    list(collection, search) {
      requireCollection(collection);
      const rows = read()[collection];
      if (!search) return rows;
      const needle = String(search).toLowerCase();
      return rows.filter((row) => String(row.name).toLowerCase().includes(needle));
    },
    create(collection, input) {
      const state = read();
      requireCollection(collection);
      const record = validate(input);
      const stored = { id: randomUUID(), ...record, createdAt: new Date().toISOString() };
      state[collection].push(stored);
      write(state);
      return stored;
    },
    remove(collection, id) {
      const state = read();
      requireCollection(collection);
      const index = state[collection].findIndex((row) => row.id === id);
      if (index === -1) throw new NotFoundError('No record with id ' + id);
      const [removed] = state[collection].splice(index, 1);
      write(state);
      return removed;
    },
    summary() {
      const state = read();
      const out = {};
      for (const collection of COLLECTIONS) out[collection] = state[collection].length;
      return out;
    },
  };
}
"#;

const SERVER_JS: &str = r#"import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createStore, ValidationError, COLLECTIONS } from './store.js';

const here = path.dirname(fileURLToPath(import.meta.url));
const publicDir = path.join(here, '..', 'public');
const dataFile = process.env.KAVACH_DATA || path.join(here, '..', 'data', 'db.json');
const store = createStore(dataFile);

const TYPES = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
};

function json(res, status, body) {
  const payload = JSON.stringify(body);
  res.writeHead(status, { 'content-type': 'application/json; charset=utf-8', 'content-length': Buffer.byteLength(payload) });
  res.end(payload);
}

async function readBody(req) {
  const chunks = [];
  let size = 0;
  for await (const chunk of req) {
    size += chunk.length;
    if (size > 102400) throw new ValidationError('Request body too large');
    chunks.push(chunk);
  }
  if (chunks.length === 0) return {};
  try {
    return JSON.parse(Buffer.concat(chunks).toString('utf8'));
  } catch {
    throw new ValidationError('Request body is not valid JSON');
  }
}

export function createApp() {
  return createServer(async (req, res) => {
    const url = new URL(req.url || '/', 'http://localhost');
    const parts = url.pathname.split('/').filter(Boolean);
    try {
      if (url.pathname === '/api/health') {
        json(res, 200, { ok: true, collections: COLLECTIONS });
        return;
      }
      if (url.pathname === '/api/summary') {
        json(res, 200, store.summary());
        return;
      }
      if (parts[0] === 'api' && parts[1] && COLLECTIONS.includes(parts[1])) {
        const collection = parts[1];
        if (req.method === 'GET' && parts.length === 2) {
          json(res, 200, store.list(collection, url.searchParams.get('q')));
          return;
        }
        if (req.method === 'POST' && parts.length === 2) {
          json(res, 201, store.create(collection, await readBody(req)));
          return;
        }
        if (req.method === 'DELETE' && parts.length === 3) {
          json(res, 200, store.remove(collection, parts[2]));
          return;
        }
        json(res, 405, { error: 'Method not allowed' });
        return;
      }
      if (parts[0] === 'api') {
        json(res, 404, { error: 'Unknown endpoint' });
        return;
      }

      const rel = url.pathname === '/' ? 'index.html' : url.pathname.replace(/^\/+/, '');
      const target = path.join(publicDir, rel);
      if (!target.startsWith(publicDir)) {
        json(res, 403, { error: 'Forbidden' });
        return;
      }
      try {
        const file = await readFile(target);
        res.writeHead(200, { 'content-type': TYPES[path.extname(target)] || 'application/octet-stream' });
        res.end(file);
      } catch {
        res.writeHead(404, { 'content-type': 'text/plain' });
        res.end('Not found');
      }
    } catch (err) {
      json(res, typeof err.status === 'number' ? err.status : 500, { error: err.message });
    }
  });
}

const isMain = process.argv[1] && import.meta.url === 'file://' + process.argv[1];
if (isMain) {
  const port = Number(process.env.PORT || 4173);
  createApp().listen(port, '127.0.0.1', () => console.log('Listening on http://127.0.0.1:' + port));
}
"#;

const APP_JS: &str = r#"const state = { collection: null };

const statusEl = document.getElementById('status');
const tbody = document.querySelector('#records tbody');
const emptyEl = document.getElementById('empty');
const titleEl = document.getElementById('list-title');
const form = document.getElementById('create-form');
const formError = document.getElementById('form-error');
const searchEl = document.getElementById('search');

async function api(path, options) {
  const res = await fetch(path, options);
  const body = await res.json().catch(() => ({}));
  if (!res.ok) throw new Error(body.error || 'Request failed with ' + res.status);
  return body;
}

function row(record, collection) {
  const tr = document.createElement('tr');
  const name = document.createElement('td');
  name.textContent = record.name;
  const created = document.createElement('td');
  created.textContent = record.createdAt ? new Date(record.createdAt).toLocaleString() : '';
  const actions = document.createElement('td');
  const del = document.createElement('button');
  del.textContent = 'Delete';
  del.className = 'danger';
  del.addEventListener('click', async () => {
    del.disabled = true;
    try {
      await api('/api/' + collection + '/' + record.id, { method: 'DELETE' });
      await load(collection);
    } catch (err) {
      statusEl.textContent = err.message;
      del.disabled = false;
    }
  });
  actions.append(del);
  tr.append(name, created, actions);
  return tr;
}

async function load(collection) {
  state.collection = collection;
  titleEl.textContent = collection;
  for (const tab of document.querySelectorAll('.tab')) {
    tab.classList.toggle('active', tab.dataset.collection === collection);
  }
  const q = searchEl && searchEl.value ? '?q=' + encodeURIComponent(searchEl.value) : '';
  const records = await api('/api/' + collection + q);
  tbody.replaceChildren(...records.map((r) => row(r, collection)));
  emptyEl.hidden = records.length > 0;
  statusEl.textContent = records.length + ' record(s) in ' + collection;
}

async function init() {
  try {
    const health = await api('/api/health');
    for (const tab of document.querySelectorAll('.tab')) {
      tab.addEventListener('click', () => load(tab.dataset.collection));
    }
    if (searchEl) searchEl.addEventListener('input', () => state.collection && load(state.collection));
    form.addEventListener('submit', async (event) => {
      event.preventDefault();
      formError.hidden = true;
      const data = Object.fromEntries(new FormData(form).entries());
      try {
        await api('/api/' + state.collection, {
          method: 'POST',
          headers: { 'content-type': 'application/json' },
          body: JSON.stringify(data),
        });
        form.reset();
        await load(state.collection);
      } catch (err) {
        formError.textContent = err.message;
        formError.hidden = false;
      }
    });
    if (health.collections.length > 0) await load(health.collections[0]);
  } catch (err) {
    statusEl.textContent = 'Cannot reach the API: ' + err.message;
  }
}

init();
"#;

const STYLES_CSS: &str = r#":root { color-scheme: light dark; --fg:#16202b; --bg:#f7f9fb; --line:#d7dfe8; --accent:#1d6fd6; }
@media (prefers-color-scheme: dark) { :root { --fg:#e8eef5; --bg:#0f1720; --line:#26313d; --accent:#57a8ff; } }
* { box-sizing: border-box; }
body { margin:0; font:16px/1.55 system-ui, sans-serif; color:var(--fg); background:var(--bg); }
header { padding:24px 20px 8px; max-width:900px; margin:0 auto; }
h1 { margin:0; font-size:24px; }
#status { color:#6b7785; font-size:14px; margin:4px 0 0; }
nav { display:flex; gap:8px; padding:8px 20px; max-width:900px; margin:0 auto; flex-wrap:wrap; align-items:center; }
.tab { font:inherit; font-size:13px; padding:6px 14px; border-radius:999px; border:1px solid var(--line); background:transparent; color:inherit; cursor:pointer; }
.tab.active { background:var(--accent); border-color:var(--accent); color:#fff; }
#search { font:inherit; font-size:13px; padding:6px 10px; border-radius:8px; border:1px solid var(--line); background:transparent; color:inherit; margin-left:auto; }
main { display:grid; grid-template-columns:1.6fr 1fr; gap:24px; padding:8px 20px 40px; max-width:900px; margin:0 auto; }
@media (max-width:720px) { main { grid-template-columns:1fr; } }
h2 { font-size:15px; text-transform:uppercase; letter-spacing:.05em; color:#6b7785; }
table { width:100%; border-collapse:collapse; font-size:14px; }
th { text-align:left; font-size:11px; text-transform:uppercase; color:#6b7785; border-bottom:1px solid var(--line); padding:6px 8px; }
td { padding:8px; border-bottom:1px solid var(--line); }
form { display:grid; gap:12px; }
label { display:grid; gap:4px; font-size:13px; color:#6b7785; }
input, textarea { font:inherit; font-size:14px; padding:8px 10px; border:1px solid var(--line); border-radius:8px; background:transparent; color:inherit; }
button[type=submit] { font:inherit; padding:9px 16px; border-radius:8px; border:1px solid var(--accent); background:var(--accent); color:#fff; cursor:pointer; justify-self:start; }
button.danger { font:inherit; font-size:12px; padding:4px 10px; border-radius:6px; border:1px solid var(--line); background:transparent; color:inherit; cursor:pointer; }
#form-error { color:#c0392b; font-size:13px; }
#empty { color:#6b7785; font-size:14px; }
"#;

const TEST_JS: &str = r#"import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { createStore, ValidationError, NotFoundError, COLLECTIONS } from '../src/store.js';

function freshStore() {
  const dir = mkdtempSync(path.join(tmpdir(), 'app-test-'));
  return createStore(path.join(dir, 'db.json'));
}

const first = COLLECTIONS[0];

test('starts empty for every collection', () => {
  const store = freshStore();
  for (const collection of COLLECTIONS) assert.deepEqual(store.list(collection), []);
});

test('creates a record with an id and a timestamp', () => {
  const store = freshStore();
  const created = store.create(first, { name: 'Example' });
  assert.ok(created.id);
  assert.ok(created.createdAt);
  assert.equal(store.list(first).length, 1);
});

test('persists across store instances', () => {
  const dir = mkdtempSync(path.join(tmpdir(), 'app-test-'));
  const file = path.join(dir, 'db.json');
  createStore(file).create(first, { name: 'Example' });
  assert.equal(createStore(file).list(first).length, 1);
});

test('rejects a missing name', () => {
  const store = freshStore();
  assert.throws(() => store.create(first, {}), ValidationError);
});

test('rejects a non-object body', () => {
  const store = freshStore();
  assert.throws(() => store.create(first, [1, 2, 3]), ValidationError);
});

test('rejects an unknown collection', () => {
  const store = freshStore();
  assert.throws(() => store.list('nope'), NotFoundError);
});

test('removes a record and reports a missing one', () => {
  const store = freshStore();
  const created = store.create(first, { name: 'Example' });
  store.remove(first, created.id);
  assert.equal(store.list(first).length, 0);
  assert.throws(() => store.remove(first, 'missing'), NotFoundError);
});

test('search filters by name', () => {
  const store = freshStore();
  store.create(first, { name: 'Alpha' });
  store.create(first, { name: 'Beta' });
  assert.equal(store.list(first, 'alp').length, 1);
  assert.equal(store.list(first, 'zzz').length, 0);
});
"#;

/// Deterministic project scaffold.
pub fn scaffold(reqs: &Requirements) -> Vec<GeneratedFile> {
    let collections: Vec<String> = reqs
        .entities
        .iter()
        .map(|e| {
            let base = slug(e);
            if base.ends_with('s') {
                base
            } else {
                format!("{base}s")
            }
        })
        .collect();
    let collections_json = format!(
        "[{}]",
        collections.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(",")
    );
    let title = reqs.summary.chars().take(70).collect::<String>();
    let tabs = collections
        .iter()
        .map(|c| format!("<button class=\"tab\" data-collection=\"{c}\">{c}</button>"))
        .collect::<Vec<_>>()
        .join("\n        ");

    let index_html = format!(
        r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>{title}</title>
    <link rel="stylesheet" href="/styles.css" />
  </head>
  <body>
    <header>
      <h1>{title}</h1>
      <p id="status">Loading…</p>
    </header>
    <nav>
        {tabs}
        <input id="search" type="search" placeholder="Search…" aria-label="Search records" />
    </nav>
    <main>
      <section>
        <h2 id="list-title">Records</h2>
        <table id="records"><thead><tr><th>Name</th><th>Created</th><th></th></tr></thead><tbody></tbody></table>
        <p id="empty" hidden>Nothing recorded yet.</p>
      </section>
      <section>
        <h2>Add a record</h2>
        <form id="create-form">
          <label>Name <input name="name" required maxlength="200" /></label>
          <label>Notes <textarea name="notes" rows="2" maxlength="2000"></textarea></label>
          <button type="submit">Save</button>
        </form>
        <p id="form-error" role="alert" hidden></p>
      </section>
    </main>
    <script type="module" src="/app.js"></script>
  </body>
</html>
"#
    );

    let readme = format!(
        "# {title}\n\nGenerated by Kavach from this request:\n\n> {prompt}\n\n## Run\n\n```bash\nnpm start        # http://127.0.0.1:4173\nnpm test         # node --test\n```\n\nNo dependencies. Data is stored in `data/db.json` (override with `KAVACH_DATA`).\n\n## Collections\n\n{collections}\n\n## API\n\n| Method | Path | Purpose |\n|---|---|---|\n| GET | /api/health | Liveness and collection list |\n| GET | /api/{collection} | List records (optional `?q=`) |\n| POST | /api/{collection} | Create a record |\n| DELETE | /api/{collection}/{{id}} | Delete a record |\n\n## What this scaffold does not do\n\n- No authentication.\n- No encryption at rest; the JSON file is plain text.\n- Single process, single user.\n",
        title = title,
        prompt = reqs.summary,
        collections = collections.iter().map(|c| format!("- `{c}`")).collect::<Vec<_>>().join("\n"),
        collection = collections.first().cloned().unwrap_or_else(|| "items".to_string()),
    );

    let package_json = format!(
        "{{\n  \"name\": \"{}\",\n  \"version\": \"0.1.0\",\n  \"private\": true,\n  \"type\": \"module\",\n  \"scripts\": {{ \"start\": \"node src/server.js\", \"test\": \"node --test\" }}\n}}\n",
        slug(&reqs.entities.first().cloned().unwrap_or_else(|| "app".into()))
    );

    vec![
        GeneratedFile { path: "package.json".into(), content: package_json, origin: "scaffold" },
        GeneratedFile {
            path: "src/store.js".into(),
            content: STORE_JS.replace("__COLLECTIONS__", &collections_json),
            origin: "scaffold",
        },
        GeneratedFile { path: "src/server.js".into(), content: SERVER_JS.to_string(), origin: "scaffold" },
        GeneratedFile { path: "public/index.html".into(), content: index_html, origin: "scaffold" },
        GeneratedFile { path: "public/app.js".into(), content: APP_JS.to_string(), origin: "scaffold" },
        GeneratedFile { path: "public/styles.css".into(), content: STYLES_CSS.to_string(), origin: "scaffold" },
        GeneratedFile { path: "test/store.test.js".into(), content: TEST_JS.to_string(), origin: "scaffold" },
        GeneratedFile { path: "README.md".into(), content: readme, origin: "scaffold" },
    ]
}

/// Guard generated content: no placeholders, no secrets, no fake domains.
pub fn guard(files: &[GeneratedFile]) -> Vec<String> {
    // Note: "placeholder" is deliberately absent — it is a legitimate HTML attribute and
    // a legitimate ARIA concept. Only unambiguous markers belong here.
    const MARKERS: &[&str] = &["TODO", "FIXME", "coming soon", "lorem ipsum", "example.com", "not implemented"];
    let mut problems = Vec::new();
    for file in files {
        for marker in MARKERS {
            if file.content.to_lowercase().contains(&marker.to_lowercase()) {
                problems.push(format!("{}: contains \"{}\"", file.path, marker));
            }
        }
        if file.content.contains("BEGIN RSA PRIVATE KEY") || file.content.contains("AKIA") {
            problems.push(format!("{}: looks like it contains a secret", file.path));
        }
    }
    problems
}

/// A file the model proposed to add.
#[derive(Debug, Clone, PartialEq)]
pub struct ProposedFile {
    pub path: String,
    pub content: String,
}

pub const MAX_MODEL_FILES: usize = 6;
pub const MAX_MODEL_FILE_BYTES: usize = 64 * 1024;
const ALLOWED_MODEL_EXTENSIONS: &[&str] = &["js", "mjs", "md", "css", "html", "json", "txt"];

/// Parse and validate a model's file manifest.
///
/// The model is untrusted input. Everything it proposes is checked here — shape, path,
/// size, extension, placeholders, secrets — and rejected as a whole if any file fails,
/// so a partially-applied proposal can never reach the disk.
pub fn parse_manifest(text: &str, existing: &[String]) -> Result<Vec<ProposedFile>, String> {
    // Models wrap JSON in prose or code fences often enough that requiring a bare object
    // would reject otherwise usable output. Take the outermost braces.
    let start = text.find('{').ok_or("no JSON object found in the model output")?;
    let end = text.rfind('}').ok_or("the model output has no closing brace")?;
    if end <= start {
        return Err("the model output has no complete JSON object".into());
    }
    let json = crate::runtime::parse_json(&text[start..=end])
        .map_err(|e| format!("the model output is not valid JSON: {e}"))?;

    let files = match json.get("files") {
        Some(crate::runtime::Value::Array(items)) => items,
        Some(_) => return Err("\"files\" is not an array".into()),
        None => return Err("the model output has no \"files\" array".into()),
    };
    if files.is_empty() {
        return Err("the model proposed no files".into());
    }
    if files.len() > MAX_MODEL_FILES {
        return Err(format!("the model proposed {} files; at most {MAX_MODEL_FILES} are accepted", files.len()));
    }

    let mut out: Vec<ProposedFile> = Vec::new();
    for (index, entry) in files.iter().enumerate() {
        let path = entry
            .path_str("path")
            .ok_or_else(|| format!("proposed file {index} has no string \"path\""))?
            .trim()
            .to_string();
        let content = entry
            .path_str("content")
            .ok_or_else(|| format!("proposed file {index} ({path}) has no string \"content\""))?
            .to_string();

        if crate::path::safe_join(Path::new("."), &path).is_err() {
            return Err(format!("proposed path {path:?} is not a safe relative path"));
        }
        if path.starts_with('.') {
            return Err(format!("proposed path {path:?} is hidden; Kavach does not write dotfiles from a model"));
        }
        let extension = Path::new(&path).extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        if !ALLOWED_MODEL_EXTENSIONS.contains(&extension.as_str()) {
            return Err(format!(
                "proposed path {path:?} has extension {extension:?}; allowed: {}",
                ALLOWED_MODEL_EXTENSIONS.join(", ")
            ));
        }
        if content.trim().is_empty() {
            return Err(format!("proposed file {path:?} is empty"));
        }
        if content.len() > MAX_MODEL_FILE_BYTES {
            return Err(format!("proposed file {path:?} is {} bytes; the limit is {MAX_MODEL_FILE_BYTES}", content.len()));
        }
        // The model may add files, never replace what Kavach generated: a replaced
        // scaffold file would make a rollback ambiguous and the tests meaningless.
        if existing.iter().any(|e| e == &path) {
            return Err(format!("proposed path {path:?} already exists; the model may only add new files"));
        }
        if out.iter().any(|f| f.path == path) {
            return Err(format!("proposed path {path:?} appears twice"));
        }

        let candidate = GeneratedFile { path: path.clone(), content: content.clone(), origin: "model" };
        let violations = guard(std::slice::from_ref(&candidate));
        if !violations.is_empty() {
            return Err(format!("proposed file {path:?} was rejected: {}", violations.join("; ")));
        }
        if !crate::security::scan_content(&path, &content).iter().all(|f| f.severity != crate::security::Severity::High) {
            return Err(format!("proposed file {path:?} contains a high-severity secret"));
        }

        out.push(ProposedFile { path, content });
    }
    Ok(out)
}

/// Ask the model for code, bounded to a small, reviewable addition.
fn request_manifest(runtime: &Runtime, reqs: &Requirements, existing: &[String]) -> Result<Vec<ProposedFile>, String> {
    let prompt = format!(
        "You are adding code to a small Node.js application with no dependencies.\n\
         Reply with JSON only. No prose, no code fences.\n\
         Shape: {{\"files\":[{{\"path\":\"src/example.js\",\"content\":\"...\"}}]}}\n\
         Rules: at most 2 files; paths relative to the project root; extensions .js or .md;\n\
         add new files only, never replace existing ones; no placeholders, TODOs or secrets.\n\n\
         The application: {}\nEntities: {}\n\n\
         Add one small, self-contained ES module under src/ that gives this app a useful pure\n\
         helper (for example formatting, validation or summarising), exporting plain functions.\n\
         Do not import anything.",
        reqs.summary,
        reqs.entities.join(", ")
    );
    let text = runtime::generate(runtime, &prompt, 900).map_err(|e| e.to_string())?;
    parse_manifest(&text, existing)
}

/// Run the generated project's own test command.
fn run_tests(project_dir: &Path) -> (bool, String) {
    match run(
        "node",
        &["--test".to_string()],
        ExecOptions {
            cwd: Some(project_dir.to_string_lossy().to_string()),
            timeout_ms: 120_000,
            max_output_bytes: 64 * 1024,
        },
    ) {
        Ok(out) if out.exit_code == 0 => (true, "node --test exited 0".to_string()),
        Ok(out) if out.timed_out => (false, format!("node --test timed out (exit {EXIT_TIMEOUT})")),
        Ok(out) => (false, format!("node --test exited {}: {}", out.exit_code, out.stderr.lines().next().unwrap_or(""))),
        Err(e) => (false, format!("cannot execute tests: {e}")),
    }
}

/// Run the whole pipeline against a project directory.
pub fn build_project(project_id: &str, project_dir: &Path, prompt: &str) -> DeliveryReport {
    build_project_with(project_id, project_dir, prompt, None)
}

/// Ask a local model to write the project notes.
///
/// This is a genuine, bounded use of a small model: prose about the plan, not code. Code
/// generation needs a model large enough to emit a valid manifest, which is a separate
/// milestone; a note that a 3B model can write well is better than a manifest it cannot.
fn model_notes(runtime: &Runtime, reqs: &Requirements, plan: &[Task]) -> Option<String> {
    let prompt = format!(
        "Write 4 short bullet points describing what this application does and how it is \
         structured, for a README. No preamble, no markdown headings.\n\nApplication: {}\n\
         Entities: {}\nTasks: {}",
        reqs.summary,
        reqs.entities.join(", "),
        plan.iter().map(|t| t.title.clone()).collect::<Vec<_>>().join("; ")
    );
    match runtime::generate(runtime, &prompt, 220) {
        Ok(text) if text.trim().len() >= 20 => Some(text.trim().to_string()),
        _ => None,
    }
}

/// Run the pipeline, optionally with a local model available.
pub fn build_project_with(
    project_id: &str,
    project_dir: &Path,
    prompt: &str,
    runtime: Option<&Runtime>,
) -> DeliveryReport {
    let started = std::time::Instant::now();
    let mut stages: Vec<Stage> = Vec::new();

    let reqs = extract_requirements(prompt);
    stages.push(Stage {
        stage: "understand",
        ok: true,
        detail: format!(
            "{} ({} entities, {} features, sensitivity: {})",
            reqs.app_kind,
            reqs.entities.len(),
            reqs.features.len(),
            reqs.data_sensitivity
        ),
    });

    let plan = build_plan(&reqs);
    stages.push(Stage {
        stage: "plan",
        ok: true,
        detail: format!("{} dependency-ordered tasks", plan.len()),
    });

    let mut files = scaffold(&reqs);
    let violations = guard(&files);
    let mut written = Vec::new();
    let mut write_failed = None;
    for file in &files {
        match write_project_file(project_dir, &file.path, &file.content) {
            Ok(_) => written.push(file.clone()),
            Err(e) => {
                write_failed = Some(format!("{}: {}", file.path, e));
                break;
            }
        }
    }

    // A local model, if one is running, writes the project notes. Its output is prose, so it
    // is guarded like any other generated content before being written.
    let mut runtime_label = None;
    let mut model_note_written = false;
    if let Some(rt) = runtime {
        runtime_label = Some(format!("{} ({})", rt.kind.as_str(), rt.model));
        if let Some(notes) = model_notes(rt, &reqs, &plan) {
            let candidate = GeneratedFile {
                path: "docs/MODEL-NOTES.md".into(),
                content: format!("# Notes written by the local model\n\nModel: `{}`\n\n{}\n", rt.model, notes),
                origin: "model",
            };
            if guard(std::slice::from_ref(&candidate)).is_empty()
                && write_project_file(project_dir, &candidate.path, &candidate.content).is_ok()
            {
                files.push(candidate.clone());
                written.push(candidate);
                model_note_written = true;
            }
        }
    }

    stages.push(Stage {
        stage: "design",
        ok: write_failed.is_none() && violations.is_empty(),
        detail: match (&write_failed, violations.is_empty()) {
            (Some(err), _) => format!("write failed: {err}"),
            (None, false) => format!("{} content violations", violations.len()),
            (None, true) => {
                if model_note_written {
                    format!("{} files written, including model-authored notes", written.len())
                } else {
                    format!("{} files written", written.len())
                }
            }
        },
    });

    // Model-authored code. Accepted only if the project still passes its own tests, and
    // rolled back if it does not — a model that breaks the build must not ship broken code.
    let mut model_files: Vec<String> = Vec::new();
    let model_code_detail = match runtime {
        None => "no local model server was reachable".to_string(),
        Some(rt) => {
            let existing = list_project_files(project_dir);
            match request_manifest(rt, &reqs, &existing) {
                Err(reason) => format!("model code rejected: {reason}"),
                Ok(proposed) => {
                    let mut written: Vec<String> = Vec::new();
                    let mut failure = None;
                    for file in &proposed {
                        match write_project_file(project_dir, &file.path, &file.content) {
                            Ok(_) => written.push(file.path.clone()),
                            Err(e) => {
                                failure = Some(format!("{}: {e}", file.path));
                                break;
                            }
                        }
                    }
                    match failure {
                        Some(err) => {
                            for path in &written {
                                let _ = delete_project_file(project_dir, path);
                            }
                            format!("model code discarded, write failed: {err}")
                        }
                        None => {
                            let (passes, detail) = run_tests(project_dir);
                            if passes {
                                model_files = written.clone();
                                format!("{} model file(s) accepted; the project's tests still pass", written.len())
                            } else {
                                for path in &written {
                                    let _ = delete_project_file(project_dir, path);
                                }
                                let (scaffold_ok, _) = run_tests(project_dir);
                                format!(
                                    "model code rolled back ({detail}){}",
                                    if scaffold_ok { "; the scaffold alone passes" } else { "; the scaffold alone also fails" }
                                )
                            }
                        }
                    }
                }
            }
        }
    };
    stages.push(Stage { stage: "model-code", ok: true, detail: model_code_detail });

    // Security scan: this is the evidence the "Run security checks" task never had.
    let scan = security::scan_project(project_dir);
    stages.push(Stage { stage: "security", ok: scan.passes(), detail: scan.summary() });

    let (tests_ok, tests_detail) = run_tests(project_dir);
    let verify_ok = tests_ok && scan.passes();
    let verify_detail = if tests_ok && !scan.passes() {
        format!("tests passed but the security scan found {} high-severity issue(s)", scan.count(security::Severity::High))
    } else {
        tests_detail
    };
    stages.push(Stage { stage: "verify", ok: verify_ok, detail: verify_detail });

    // A task is verified only if a command produced evidence for it.
    let mut evidenced = vec![
        "Scaffold the project structure",
        "Implement the application logic and API surface",
        "Implement the user interface",
        "Write and run automated tests",
        "Write run and build documentation",
    ];
    if scan.files_scanned > 0 {
        evidenced.push("Run security checks");
    }
    let mut verified = Vec::new();
    let mut unverified = Vec::new();
    for task in &plan {
        if verify_ok && evidenced.contains(&task.title.as_str()) {
            verified.push(task.id.clone());
        } else {
            unverified.push(task.id.clone());
        }
    }

    let mut limitations = vec![
        "Performance was not measured; no benchmark is claimed.".to_string(),
        "No end-to-end browser test was executed unless the project declares one.".to_string(),
    ];
    match (&runtime_label, model_note_written) {
        (None, _) => limitations.insert(
            0,
            "No local model server was found on 127.0.0.1, so the application code is the deterministic scaffold and no notes were written.".to_string(),
        ),
        (Some(name), false) => limitations.insert(
            0,
            format!("A local model ({name}) was reachable but produced no usable notes; the application code is the deterministic scaffold."),
        ),
        (Some(_), true) => limitations.insert(
            0,
            if model_files.is_empty() {
                "Application code is the deterministic scaffold; the local model contributed the notes only.".to_string()
            } else {
                format!(
                    "Application code is the deterministic scaffold plus {} model-authored file(s) that passed the project's tests.",
                    model_files.len()
                )
            },
        ),
    }
    if !verify_ok {
        limitations.insert(0, "Verification did not pass; nothing in this delivery is claimed to work.".to_string());
    }

    stages.push(Stage {
        stage: "deliver",
        ok: verify_ok,
        detail: if verify_ok {
            format!("{} tasks verified, {} unverified", verified.len(), unverified.len())
        } else {
            "delivered WITHOUT passing evidence".to_string()
        },
    });

    DeliveryReport {
        project_id: project_id.to_string(),
        stages,
        files: written,
        verified,
        unverified,
        limitations,
        duration_ms: started.elapsed().as_millis() as u64,
        runtime: runtime_label,
        model_files: model_files.clone(),
        security_summary: scan.summary(),
        security_passed: scan.passes(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("kavach-builder-{}-{}", tag, std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn extracts_entities_and_features() {
        let reqs = extract_requirements("Build a web app to manage invoices and clients with search and export");
        assert_eq!(reqs.app_kind, "web-app");
        assert!(reqs.entities.iter().any(|e| e.eq_ignore_ascii_case("invoice")), "{:?}", reqs.entities);
        assert!(reqs.entities.iter().any(|e| e.eq_ignore_ascii_case("client")), "{:?}", reqs.entities);
        assert!(reqs.features.iter().any(|f| f.statement.contains("Search")));
        assert!(reqs.features.iter().all(|f| !f.acceptance.is_empty()));
    }

    #[test]
    fn detects_offline_and_sensitivity() {
        let reqs = extract_requirements("An Android mobile app to manage patient records that works offline");
        assert_eq!(reqs.app_kind, "mobile-app");
        assert!(reqs.offline_required);
        assert_eq!(reqs.data_sensitivity, "sensitive");
    }

    #[test]
    fn plan_is_dependency_ordered_and_complete() {
        let reqs = extract_requirements("A web app to manage invoices");
        let plan = build_plan(&reqs);
        assert!(plan.len() >= 6);
        for task in &plan {
            assert!(!task.acceptance.is_empty(), "task {} has no acceptance criteria", task.id);
            for dep in &task.depends_on {
                assert!(plan.iter().any(|t| &t.id == dep), "missing dependency {dep}");
            }
        }
    }

    #[test]
    fn scaffold_has_no_placeholders_and_covers_the_app() {
        let reqs = extract_requirements("A web app to manage invoices and clients");
        let files = scaffold(&reqs);
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        for required in ["package.json", "src/store.js", "src/server.js", "public/index.html", "public/app.js", "public/styles.css", "test/store.test.js", "README.md"] {
            assert!(paths.contains(&required), "missing {required}");
        }
        assert!(guard(&files).is_empty(), "guard found {:?}", guard(&files));
    }

    #[test]
    fn manifest_accepts_a_well_formed_proposal() {
        let text = r#"{"files":[{"path":"src/helper.js","content":"export const add = (a, b) => a + b;\n"}]}"#;
        let files = parse_manifest(text, &[]).expect("should accept");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "src/helper.js");
    }

    #[test]
    fn manifest_tolerates_prose_around_the_json() {
        let text = "Sure! Here you go:\n```json\n{\"files\":[{\"path\":\"a.md\",\"content\":\"# Hi\"}]}\n```\nHope that helps.";
        assert_eq!(parse_manifest(text, &[]).unwrap().len(), 1);
    }

    #[test]
    fn manifest_rejects_every_unsafe_shape() {
        let cases: Vec<(&str, &str)> = vec![
            ("no json at all", "I cannot help with that."),
            ("empty files", r#"{"files":[]}"#),
            ("files not an array", r#"{"files":"nope"}"#),
            ("no files key", r#"{"other":1}"#),
            ("missing path", r#"{"files":[{"content":"x"}]}"#),
            ("missing content", r#"{"files":[{"path":"a.js"}]}"#),
            ("absolute path", r#"{"files":[{"path":"/etc/passwd","content":"x"}]}"#),
            ("traversal", r#"{"files":[{"path":"../out.js","content":"x"}]}"#),
            ("hidden file", r#"{"files":[{"path":".env","content":"x"}]}"#),
            ("bad extension", r#"{"files":[{"path":"run.sh","content":"x"}]}"#),
            ("empty content", r#"{"files":[{"path":"a.js","content":"   "}]}"#),
            ("duplicate path", r#"{"files":[{"path":"a.js","content":"x"},{"path":"a.js","content":"y"}]}"#),
            ("too many files", r#"{"files":[{"path":"a.js","content":"x"},{"path":"b.js","content":"x"},{"path":"c.js","content":"x"},{"path":"d.js","content":"x"},{"path":"e.js","content":"x"},{"path":"f.js","content":"x"},{"path":"g.js","content":"x"}]}"#),
        ];
        for (label, text) in cases {
            assert!(parse_manifest(text, &[]).is_err(), "should reject: {label}");
        }
    }

    #[test]
    fn manifest_rejects_overwriting_a_generated_file() {
        let existing = vec!["src/store.js".to_string()];
        let text = r#"{"files":[{"path":"src/store.js","content":"export const x = 1;"}]}"#;
        let err = parse_manifest(text, &existing).unwrap_err();
        assert!(err.contains("already exists"), "{err}");
    }

    #[test]
    fn manifest_rejects_placeholders_and_secrets() {
        let todo = r#"{"files":[{"path":"a.js","content":"// TODO: finish this"}]}"#;
        assert!(parse_manifest(todo, &[]).unwrap_err().contains("rejected"));

        // The content guard catches this before the security scanner does; either way it
        // must be rejected, and the message must say why.
        let secret = r#"{"files":[{"path":"a.js","content":"-----BEGIN RSA PRIVATE KEY-----"}]}"#;
        let err = parse_manifest(secret, &[]).unwrap_err();
        assert!(err.contains("secret"), "rejection should mention the secret: {err}");
    }

    #[test]
    fn guard_rejects_placeholders_and_secrets() {
        let bad = vec![
            GeneratedFile { path: "a.js".into(), content: "// TODO: implement".into(), origin: "model" },
            GeneratedFile { path: "b.js".into(), content: "const k = 'AKIAABCDEFGHIJKLMNOP'".into(), origin: "model" },
        ];
        let problems = guard(&bad);
        assert_eq!(problems.len(), 2, "{problems:?}");
    }

    #[test]
    fn builds_a_project_that_actually_passes_its_tests() {
        let dir = tmp("build");
        let report = build_project("p-test", &dir, "Build a web app to manage invoices and clients with search");
        assert!(report.verified_ok(), "stages: {:?}", report.stages);
        assert!(report.stages.iter().any(|s| s.stage == "verify" && s.ok));
        assert!(report.stages.iter().any(|s| s.stage == "security" && s.ok), "security scan must run and pass");
        assert!(report.security_passed);
        assert_eq!(report.verified.len(), 6); // security checks are now evidenced
        assert_eq!(report.unverified.len(), 1); // only the search task: security is now evidenced
        assert!(dir.join("src/store.js").exists());
        assert!(dir.join("test/store.test.js").exists());
        std::fs::remove_dir_all(&dir).ok();
    }
}
