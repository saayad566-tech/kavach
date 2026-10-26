// Renders the desktop UI in headless Chromium against a stubbed Tauri IPC.
//
// The real shell cannot run in this container (it needs crates.io and a system webview),
// but the UI is plain HTML and JavaScript: given a stub that answers the same envelopes the
// Rust commands return, it must render without a single JavaScript error. That is what this
// checks — no more, and it is labelled as such in the report.

import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { spawn } from 'node:child_process';
import { extname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const UI = fileURLToPath(new URL('../apps/desktop/ui/', import.meta.url));
const TYPES = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css' };

const STUB = `
window.__TAURI__ = {
  invoke: async (command, args) => {
    switch (command) {
      case 'workspace_root':
        return { ok: true, path: 'C:/Users/test/AppData/Roaming/app.kavach.builder' };
      case 'probe_hardware':
        return { ok: true, platform: 'windows', arch: 'x86_64', cpuModel: 'Test CPU', logicalCores: 8,
                 instructionSets: ['avx2'], ramTotalBytes: 17179869184, ramAvailableBytes: 12884901888,
                 gpus: [{ vendor: 'nvidia', name: 'Test GPU', vramBytes: 8589934592, acceleration: ['cuda'], unifiedMemory: false }],
                 diskTotalBytes: 536870912000, diskAvailableBytes: 300647710720,
                 unmeasured: ['Sustained memory bandwidth (never measured)'], sources: {} };
      case 'list_projects':
        return { ok: true, projects: [{ id: 'p-1', name: 'Invoice app', appKind: 'web-app', createdAt: '1', verifiedTasks: 5, unverifiedTasks: 2 }] };
      case 'extract_requirements':
        return { ok: true, appKind: 'web-app', summary: 'Build a web app to manage invoices',
                 entities: ['Invoice', 'Client'], dataSensitivity: 'internal', offlineRequired: false,
                 assumptions: ['Data is stored locally in a JSON file'],
                 features: [{ id: 'FR-01', statement: 'CRUD for Invoice', acceptance: 'A invoice can be created' }],
                 plan: [{ id: 'T-01', title: 'Scaffold', acceptance: ['tree matches'], dependsOn: [] }] };
      case 'create_project':
        return { ok: true, id: 'p-2', name: 'Invoice app', appKind: 'web-app', createdAt: '2' };
      case 'build_project':
        return { ok: true, report: {
          projectId: 'p-2', durationMs: 1200,
          stages: [{ stage: 'understand', ok: true, detail: 'web-app' },
                   { stage: 'verify', ok: true, detail: 'node --test exited 0' },
                   { stage: 'deliver', ok: true, detail: '5 tasks verified, 2 unverified' }],
          files: [{ path: 'src/store.js', origin: 'scaffold' }],
          tasksVerified: ['T-01','T-02','T-03','T-04','T-05'], tasksUnverified: ['T-06','T-07'],
          limitations: ['No local model is wired into the desktop shell yet'],
          checkpointId: 'cp-1' } };
      case 'list_checkpoints':
        return { ok: true, checkpoints: [{ id: 'cp-1', label: 'before build', treeHash: 'abcdef0123456789', createdAt: '3' }] };
      default:
        return { ok: false, error: 'stub does not implement ' + command, kind: 'unknown-command' };
    }
  }
};
`;

const server = createServer(async (req, res) => {
  const path = (req.url || '/').split('?')[0];
  if (path === '/' || path === '/index.html') {
    const html = await readFile(join(UI, 'index.html'), 'utf8');
    // Inject the stub before the app script so the UI sees an IPC on first run.
    const withStub = html.replace('<script src="app.js"></script>', `<script>${STUB}</script><script src="app.js"></script>`);
    res.writeHead(200, { 'content-type': 'text/html' });
    res.end(withStub);
    return;
  }
  try {
    const file = await readFile(join(UI, path.replace(/^\/+/, '')));
    res.writeHead(200, { 'content-type': TYPES[extname(path)] ?? 'application/octet-stream' });
    res.end(file);
  } catch {
    res.writeHead(404).end('not found');
  }
});

await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
const port = server.address().port;

function chromium(args, url) {
  return new Promise((resolve) => {
    const child = spawn('chromium', ['--headless=new', '--disable-gpu', '--no-sandbox', '--virtual-time-budget=8000', ...args, url]);
    let out = '';
    let err = '';
    child.stdout.on('data', (d) => (out += d));
    child.stderr.on('data', (d) => (err += d));
    child.on('close', () => resolve({ out, err }));
  });
}

const { out: dom } = await chromium(['--dump-dom'], `http://127.0.0.1:${port}/`);

const checks = {
  'page rendered': dom.includes('Kavach') && dom.length > 1500,
  'shell badge shows a connected shell': /desktop shell · local only/.test(dom),
  'device panel is populated': /Test CPU/.test(dom) && /16\.0 GiB/.test(dom),
  'unmeasured items are listed': /never measured/.test(dom),
  'projects list rendered': /Invoice app/.test(dom),
  'local inference is declared unimplemented': /Local inference is not implemented in this build/.test(dom),
  // The element always exists; what matters is that it carries no message.
  'no error banner': (() => {
    const match = dom.match(/<p id="error"[^>]*>([\s\S]*?)<\/p>/);
    return match !== null && match[1].trim() === '';
  })(),
};

for (const [name, ok] of Object.entries(checks)) console.log(`  ${ok ? 'PASS' : 'FAIL'}  ${name}`);

server.close();
const failed = Object.values(checks).filter((v) => !v).length;
console.log(`\n${Object.keys(checks).length - failed}/${Object.keys(checks).length} checks passed`);
console.log(failed === 0 ? 'RESULT: PASS' : 'RESULT: FAIL');
process.exit(failed === 0 ? 0 : 1);
