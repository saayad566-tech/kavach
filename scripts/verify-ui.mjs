// Renders the desktop UI in headless Chromium against a stubbed Tauri IPC.
//
// The real shell cannot run in this container (it needs a system webview), but the UI is
// plain HTML and JavaScript: given a stub that answers the same envelopes the Rust commands
// return, it must render without a JavaScript error. That is all this checks — and it is
// labelled as such in the report.
//
// Two rules learned the hard way:
//   1. Assertions run against the DOM with <script> blocks removed. Otherwise a check can
//      pass by matching the text of the stub instead of anything the app produced.
//   2. The page is driven (a build is clicked) before asserting, so the report, files and
//      actions are actually exercised rather than merely present in markup.

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
    const report = {
      projectId: 'p-1',
      stages: [
        { stage: 'understand', ok: true, detail: 'web-app, 2 entities' },
        { stage: 'plan', ok: true, detail: '8 tasks' },
        { stage: 'design', ok: true, detail: '14 files written' },
        { stage: 'model-code', ok: true, detail: '1 model file accepted' },
        { stage: 'security', ok: true, detail: '8 file(s) scanned, no findings' },
        { stage: 'verify', ok: true, detail: 'node --test exited 0' },
      ],
      files: [{ path: 'src/server.js', origin: 'scaffold' }, { path: 'src/summarise.js', origin: 'model' }],
      tasksVerified: ['T-01', 'T-02', 'T-03', 'T-04', 'T-05', 'T-06'],
      tasksUnverified: ['T-07'],
      limitations: ['Application code is the deterministic scaffold'],
      durationMs: 1200,
      modelFiles: ['src/summarise.js'],
      runtime: 'ollama (qwen2.5-coder:7b)',
      securitySummary: '8 file(s) scanned, no findings',
      securityPassed: true,
    };

    switch (command) {
      case 'workspace_root':
        return { ok: true, path: '/tmp/ws' };
      case 'probe_hardware':
        return { ok: true, platform: 'windows', arch: 'x86_64', cpuModel: 'Test CPU', logicalCores: 8,
                 instructionSets: ['avx2'], ramTotalBytes: 17179869184, ramAvailableBytes: 12884901888,
                 gpus: [{ vendor: 'nvidia', name: 'Test GPU', vramBytes: 8589934592, acceleration: ['cuda'], unifiedMemory: false }],
                 diskTotalBytes: 536870912000, diskAvailableBytes: 300647710720,
                 unmeasured: ['Sustained memory bandwidth (never measured)', 'Battery and power state'],
                 sources: {} };
      case 'list_projects':
        return { ok: true, projects: [{ id: 'p-1', name: 'Invoice app', appKind: 'web-app', createdAt: '1',
                 verifiedTasks: 6, unverifiedTasks: 1, rootPath: '/tmp/ws/projects/p-1' }] };
      case 'probe_runtime':
        return { ok: true, available: true, kind: 'ollama', host: '127.0.0.1', port: 11434,
                 model: 'qwen2.5-coder:7b', models: ['qwen2.5-coder:7b'] };
      case 'create_project':
        return { ok: true, id: 'p-1', name: 'Invoice app', appKind: 'web-app', createdAt: '1',
                 rootPath: '/tmp/ws/projects/p-1' };
      case 'extract_requirements':
        return { ok: true, appKind: 'web-app', summary: 'Build a web app to manage invoices',
                 entities: ['Invoice', 'Client'], dataSensitivity: 'internal', offlineRequired: false,
                 assumptions: ['Data is stored locally'],
                 features: [{ id: 'FR-01', statement: 'CRUD for Invoice', acceptance: 'A invoice can be created' }],
                 plan: [{ id: 'T-01', title: 'Scaffold', acceptance: ['tree matches'], dependsOn: [] }] };
      case 'build_project':
        return { ok: true, report };
      case 'security_scan':
        return { ok: true, filesScanned: 14, passed: true, summary: '14 file(s) scanned, no findings', findings: [] };
      case 'create_checkpoint':
        return { ok: true, id: 'cp-9', label: 'manual checkpoint', treeHash: 'abc123', createdAt: 1 };
      case 'read_project_file':
        return { ok: true, path: args && args.relPath, content: 'export const x = 1;', bytes: 20 };
      case 'list_files':
        return { ok: true, files: ['package.json', 'src/server.js', 'src/analytics.js', 'README.md'] };
      case 'export_project':
        return { ok: true, destination: '/tmp/out', files: 14, bytes: 4096 };
      case 'disk_usage':
        return { ok: true, totalBytes: 536870912000, availableBytes: 300647710720 };
      case 'install_model':
        return { ok: true, modelId: args && args.modelId, path: '/tmp/ws/models/model.gguf',
                 sizeBytes: 4404019200, sha256: 'a'.repeat(64), licenceName: 'Apache-2.0' };
      case 'hash_file':
        return { ok: true, sha256: 'b'.repeat(64) };
      case 'verify_model':
        return { ok: true, matches: true };
      case 'model_catalog':
        return { ok: true, budgetBytes: 24051816857,
                 specs: [{ id: 'qwen2.5-coder-7b', name: 'Qwen2.5 Coder 7B', family: 'qwen2.5-coder',
                           licence: 'Apache-2.0', strengths: 'the best code model that fits a laptop',
                           paramsB: 70, context: 32768, bestQuant: 'Q5_K_M', sizeBytes: 5046586572 }],
                 quants: [{ id: 'Q4_K_M', quality: 88, note: 'default' }],
                 recommended: [
                   { id: 'qwen2.5-coder-32b', name: 'Qwen2.5 Coder 32B', quant: 'Q4_K_M',
                     verdict: 'tight', sizeBytes: 19241453486, context: 32768, tokensPerSecond: 154 },
                   { id: 'qwen2.5-coder-7b', name: 'Qwen2.5 Coder 7B', quant: 'Q8_0',
                     verdict: 'comfortable', sizeBytes: 7423918080, context: 32768, tokensPerSecond: 1280 },
                 ] };
      case 'models_list':
        return { ok: true, totalBytes: 5046586572, active: 'm-1',
                 models: [
                   { id: 'm-1', specId: 'qwen2.5-coder-7b', quant: 'Q4_K_M', path: '/m/a.gguf',
                     sizeBytes: 5046586572, sha256: 'aa', licence: 'Apache-2.0', addedAt: '1',
                     active: true, present: true },
                   { id: 'm-2', specId: 'llama-3.2-3b', quant: 'Q5_K_M', path: '/m/b.gguf',
                     sizeBytes: 2147483648, sha256: 'bb', licence: 'Llama 3.2 Community', addedAt: '2',
                     active: false, present: false },
                 ] };
      case 'model_activate':
        return { ok: true, active: args && args.id };
      case 'model_forget':
        return { ok: true, forgot: args && args.id };
      case 'model_inspect':
        return { ok: true, path: args && args.path, sizeBytes: 4, looksLikeAModel: false, problem: 'not a GGUF file' };
      case 'project_memory':
        return { ok: true,
                 tasks: [
                   { id: 'stage:verify', title: 'Pipeline stage: verify', state: 'verified',
                     acceptance: ['the stage reports a result'], evidence: ['1 test file(s) passed under the permission model'], updatedAt: '1' },
                   { id: 'T-07', title: 'Plan task T-07', state: 'needs-verification',
                     acceptance: ['verify passes'], evidence: [], updatedAt: '1' },
                 ],
                 notes: [{ id: 'n-1', kind: 'change', text: 'Build: 14 files', evidence: 'no findings', createdAt: '1' }] };
      case 'list_checkpoints':
        return { ok: true, checkpoints: [{ id: 'cp-1', label: 'before build', treeHash: 'abcdef0123456789', createdAt: '3' }] };
      case 'restore_checkpoint':
        return { ok: true, restored: { restored: ['src/server.js'], removed: [] } };
      default:
        return { ok: false, error: 'stub does not implement ' + command, kind: 'unknown-command' };
    }
  }
};`;

const DRIVER = `
window.addEventListener('load', () => {
  setTimeout(() => {
    const build = document.getElementById('build');
    if (build) build.click();
  }, 200);
});`;

const server = createServer(async (req, res) => {
  const path = (req.url || '/').split('?')[0];

  if (path === '/' || path === '/index.html') {
    const html = await readFile(join(UI, 'index.html'), 'utf8');
    const injected = html.replace(
      '<script src="app.js"></script>',
      `<script>${STUB}</script><script>${DRIVER}</script><script src="app.js"></script>`,
    );
    res.writeHead(200, { 'content-type': 'text/html' });
    res.end(injected);
    return;
  }
  try {
    const file = await readFile(join(UI, path.replace(/^\/+/, '')));
    res.writeHead(200, { 'content-type': TYPES[extname(path)] ?? 'application/octet-stream' });
    res.end(file);
  } catch {
    res.writeHead(404);
    res.end();
  }
});

const port = await new Promise((resolve) => {
  server.listen(0, '127.0.0.1', () => resolve(server.address().port));
});

function chromium(url) {
  return new Promise((resolve) => {
    const child = spawn('chromium', [
      '--headless=new', '--disable-gpu', '--no-sandbox', '--virtual-time-budget=12000', '--dump-dom', url,
    ]);
    let out = '';
    child.stdout.on('data', (d) => { out += d; });
    child.on('close', () => resolve(out));
  });
}

const rawDom = await chromium(`http://127.0.0.1:${port}/`);
server.close();

// Assertions run on rendered markup only, never on script text.
const dom = rawDom.replace(/<script[\s\S]*?<\/script>/g, '');
const text = dom;

const checks = {
  'page rendered': /Kavach/.test(text),
  'shell badge shows a connected shell': /desktop shell/.test(text),
  'device panel is populated': /This device/.test(text) && /Test CPU/.test(text),
  'unmeasured items are listed': /never measured/.test(text),
  'projects list rendered': /Invoice app/.test(text),
  'project rows are openable': /<button class="link"[^>]*>[^<]*verified/.test(dom),
  'runtime status is reported': /Local model connected: qwen2\.5-coder:7b/.test(text),
  'build report rendered': /node --test exited 0/.test(text),
  'security result is shown': /8 file\(s\) scanned, no findings/.test(text),
  'model contribution is shown': /Model-authored code accepted: src\/summarise\.js/.test(text),
  'checkpoint action is reachable': /id="checkpoint"/.test(dom) && !/id="checkpoint"[^>]*disabled/.test(dom),
  'rescan action is reachable': /id="rescan"/.test(dom) && !/id="rescan"[^>]*disabled/.test(dom),
  'files are listed': /src\/analytics\.js/.test(text),
  'ledger shows task states': /2 task\(s\):/.test(text) && /needs-verification/.test(text),
  'ledger shows evidence': /passed under the permission model/.test(text),
  'notes are shown': /change: Build: 14 files/.test(text),
  'catalogue shows a budget for this machine': /Budget for model weights: /.test(text),
  'recommendations show a fit verdict': /comfortable/.test(text) && /tight/.test(text),
  'a 32B model is recommended on a big machine': /Qwen2\.5 Coder 32B/.test(text),
  'installed models are listed': /qwen2\.5-coder-7b/.test(text) && /active/.test(text),
  'a missing model file is flagged': /file missing/.test(text),
  'installed total is shown': /2 model\(s\), .* on disk/.test(text),
  'a file is inspected before it is hashed': /id="verify-form"/.test(dom),
  'model panel explains the licence gate': /checks\s+the licence and the hash/.test(text),
  'disk space is reported': /Disk: .* free of .*/.test(text),
  'model install form is present': /id="model-source"/.test(dom) && /id="model-sha"/.test(dom),
  'licence acceptance is required': /id="model-accept"[^>]*required/.test(dom),
  'model file input is labelled': /<label for="model-source"/.test(dom),
  'page declares a language': /<html[^>]+lang="en"/.test(dom),
  'skip link present': /class="skip-link"[^>]*href="#main"/.test(dom),
  'status region is announced': /role="status"[^>]*aria-live="polite"|aria-live="polite"[^>]*role="status"/.test(dom),
  'every input has a label': (() => {
    const inputs = [...dom.matchAll(/<(input|textarea)\b[^>]*>/g)].map((m) => m[0]);
    return inputs.every((tag) => /id="/.test(tag)) && (dom.match(/<label/g) || []).length >= inputs.length;
  })(),
  'focus styles defined': /:focus-visible/.test(await readFile(join(UI, 'styles.css'), 'utf8')),
  'no error banner': (() => {
    const match = dom.match(/<p id="error"[^>]*>([\s\S]*?)<\/p>/);
    return match !== null && match[1].trim() === '';
  })(),
};

let failed = 0;
for (const [name, ok] of Object.entries(checks)) {
  console.log(`  ${ok ? 'PASS' : 'FAIL'}  ${name}`);
  if (!ok) failed += 1;
}
const total = Object.keys(checks).length;
console.log(`\n${total - failed}/${total} checks passed`);
if (failed > 0) {
  console.log('RESULT: FAIL');
  process.exit(1);
}
console.log('RESULT: PASS');
