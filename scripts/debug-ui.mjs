// Debug helper: render the desktop UI with the stub and print any page error.
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { spawn } from 'node:child_process';
import { join, extname } from 'node:path';

const UI = 'apps/desktop/ui';
const TYPES = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css' };

const stub = `
window.__TAURI__ = { invoke: async (command, args = {}) => {
  switch (command) {
    case 'workspace_root': return { ok: true, path: '/tmp/ws' };
    case 'probe_hardware': return { ok: true, platform: 'windows', arch: 'x86_64', cpuModel: 'Test CPU',
      physicalCores: 4, logicalCores: 8, ramTotalBytes: 17179869184, ramAvailableBytes: 12884901888,
      gpus: [{ vendor: 'test', name: 'Test GPU', vramBytes: 8589934592, acceleration: 'vulkan' }],
      disk: { totalBytes: 512000000000, availableBytes: 256000000000 },
      instructionSets: ['avx2'], unmeasured: ['battery', 'bandwidth'], sources: {}, probedAt: '1' };
    case 'list_projects': return { ok: true, projects: [
      { id: 'p-1', name: 'Invoice app', appKind: 'web-app', verifiedTasks: 6, unverifiedTasks: 2, rootPath: '/tmp/ws/projects/p-1' } ] };
    case 'probe_runtime': return { ok: true, available: true, kind: 'ollama', host: '127.0.0.1', port: 11434, model: 'qwen2.5-coder:7b', models: ['qwen2.5-coder:7b'] };
    case 'create_checkpoint': return { ok: true, id: 'cp-1', label: 'x', treeHash: 'h', createdAt: 1 };
    case 'security_scan': return { ok: true, filesScanned: 14, passed: true, summary: '14 file(s) scanned, no findings', findings: [] };
    case 'read_project_file': return { ok: true, path: args.relPath, content: '// file', bytes: 8 };
    case 'list_files': return { ok: true, files: ['package.json', 'src/server.js'] };
    case 'export_project': return { ok: true, destination: '/out', files: 8, bytes: 1234 };
    case 'create_project': return { ok: true, id: 'p-1', name: 'Invoice app', appKind: 'web-app', createdAt: '1', rootPath: '/tmp/ws/projects/p-1' };
    case 'extract_requirements': return { ok: true, requirements: { appKind: 'web-app', summary: 'x', entities: [], features: [], assumptions: [] } };
    case 'build_project': return { ok: true, report: { projectId: 'p-1', stages: [{ stage: 'understand', ok: true, detail: 'ok' }],
      files: [{ path: 'src/server.js', origin: 'scaffold' }], tasksVerified: ['a'], tasksUnverified: [], limitations: [],
      durationMs: 1000, modelFiles: [], runtime: null, securitySummary: '14 file(s) scanned, no findings', securityPassed: true } };
    default: return { ok: false, error: 'unknown command ' + command, kind: 'unknown-command' };
  }
} };`;

const server = createServer(async (req, res) => {
  const name = req.url === '/' ? 'index.html' : req.url.split('?')[0].replace(/^\//, '');
  try {
    let body = await readFile(join(UI, name), 'utf8');
    if (name === 'index.html') {
      body = body.replace('<script src="app.js"></script>', `<script>${stub}</script><script src="app.js"></script>`);
    }
    res.writeHead(200, { 'content-type': TYPES[extname(name)] || 'application/octet-stream' });
    res.end(body);
  } catch {
    res.writeHead(404);
    res.end();
  }
});

await new Promise((resolve) => server.listen(4477, resolve));

const child = spawn('chromium', [
  '--headless=new', '--disable-gpu', '--no-sandbox', '--virtual-time-budget=8000',
  '--enable-logging=stderr', '--v=0', '--dump-dom', 'http://127.0.0.1:4477/',
]);

let out = '';
let err = '';
child.stdout.on('data', (d) => { out += d; });
child.stderr.on('data', (d) => { err += d; });

await new Promise((resolve) => child.on('close', resolve));
server.close();

const banner = out.match(/<p id="error"[^>]*>([\s\S]*?)<\/p>/);
console.log('error banner:', banner ? JSON.stringify(banner[1].trim()) : '(none)');
const badge = out.match(/<span id="host-badge"[^>]*>([\s\S]*?)<\/span>/);
console.log('badge:', badge ? JSON.stringify(badge[1].trim()) : '(none)');
const consoleErrors = err.split('\n').filter((l) => /error|Error|Uncaught/.test(l)).slice(0, 6);
console.log('console:', consoleErrors.length ? consoleErrors.join('\n  ') : '(clean)');
