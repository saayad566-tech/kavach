// Kavach desktop UI.
//
// Plain JavaScript, no bundler: the app is a static page and every privileged action is a
// Tauri command. `invoke` returns the command's JSON envelope, so errors are rendered
// rather than thrown into the console.

const invoke = window.__TAURI__?.invoke;

const el = (id) => document.getElementById(id);
const state = { workspace: null, projectId: null, projectDir: null, runtime: null };

function setBadge(text, tone) {
  const badge = el('host-badge');
  badge.textContent = text;
  badge.className = `pill ${tone ?? ''}`;
}

function showError(message) {
  const box = el('error');
  box.textContent = message;
  box.hidden = !message;
}

function human(bytes) {
  if (!bytes) return 'not reported';
  const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB'];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(1)} ${units[unit]}`;
}

async function call(command, args = {}) {
  if (!invoke) throw new Error('Tauri IPC is unavailable — this page must run inside the desktop shell.');
  const envelope = await invoke(command, args);
  if (!envelope || envelope.ok !== true) {
    const error = new Error(envelope?.error ?? `command ${command} failed`);
    error.kind = envelope?.kind;
    throw error;
  }
  return envelope;
}

async function boot() {
  if (!invoke) {
    setBadge('not running in the desktop shell', 'bad');
    showError('This page is the Kavach desktop UI. Launch the installed application rather than opening the file directly.');
    return;
  }
  try {
    const workspace = await call('workspace_root');
    state.workspace = workspace.path;
    setBadge('desktop shell · local only', 'ok');
    await loadDevice();
    await loadRuntime();
    await loadModels();
    await loadCatalog();
    await loadInstalledModels();
    await loadProjects();
  } catch (err) {
    setBadge('shell error', 'bad');
    showError(err.message);
  }
}

async function loadDevice() {
  try {
    const info = await call('probe_hardware', { workdir: state.workspace });
    el('device-sub').textContent = `${info.platform} / ${info.arch} · probed ${new Date().toLocaleTimeString()}`;
    const facts = [
      ['CPU', info.cpuModel || 'not reported'],
      ['Logical cores', String(info.logicalCores)],
      ['RAM total', human(info.ramTotalBytes)],
      ['RAM available', human(info.ramAvailableBytes)],
      ['GPU', info.gpus.length ? info.gpus.map((g) => g.name).join(', ') : 'none reported'],
      ['Disk free', `${human(info.diskAvailableBytes)} of ${human(info.diskTotalBytes)}`],
      ['Instruction sets', info.instructionSets.length ? info.instructionSets.join(', ') : 'not reported'],
    ];
    el('device-facts').replaceChildren(
      ...facts.flatMap(([label, value]) => {
        const dt = document.createElement('dt');
        dt.textContent = label;
        const dd = document.createElement('dd');
        dd.textContent = value;
        const wrap = document.createElement('div');
        wrap.append(dt, dd);
        return [wrap];
      }),
    );
    el('unmeasured').replaceChildren(
      ...info.unmeasured.map((item) => {
        const li = document.createElement('li');
        li.textContent = item;
        return li;
      }),
    );
  } catch (err) {
    el('device-sub').textContent = `probe failed: ${err.message}`;
  }
}

// Local model discovery. Absence is reported plainly, never hidden behind a spinner.
async function loadRuntime() {
  const notice = el('inference-notice');
  try {
    const found = await call('probe_runtime');
    state.runtime = found.available ? found : null;
    if (found.available) {
      notice.className = 'notice notice--ok';
      notice.innerHTML =
        `<strong>Local model connected: ${found.model}</strong> (${found.kind} on ${found.host}:${found.port}). ` +
        'It writes the project notes. The application code is still Kavach\'s deterministic scaffold, ' +
        'because model-authored code generation needs a larger model than this milestone targets.';
    } else {
      notice.className = 'notice';
      notice.innerHTML =
        '<strong>No local model is running.</strong> The application code comes from Kavach\'s ' +
        'deterministic scaffold, so it is the same every time. Start Ollama or a llama.cpp server ' +
        'and build again to have a model write the project notes.';
    }
  } catch (err) {
    notice.className = 'notice';
    notice.textContent = `Could not check for a local model: ${err.message}`;
  }
}

async function showRequirements() {
  showError('');
  try {
    const reqs = await call('extract_requirements', { prompt: el('prompt').value });
    el('requirements-card').hidden = false;
    el('req-sub').textContent = `${reqs.appKind} · sensitivity: ${reqs.dataSensitivity} · offline: ${reqs.offlineRequired ? 'required' : 'not required'}`;
    const parts = [];
    const list = (title, items) => {
      const h = document.createElement('h3');
      h.textContent = title;
      const ul = document.createElement('ul');
      ul.className = 'tight';
      ul.append(
        ...items.map((text) => {
          const li = document.createElement('li');
          li.textContent = text;
          return li;
        }),
      );
      parts.push(h, ul);
    };
    list('Entities', reqs.entities);
    list('Features', reqs.features.map((f) => `${f.id}: ${f.statement} — ${f.acceptance}`));
    list('Assumptions', reqs.assumptions);
    list('Plan', reqs.plan.map((t) => `${t.id}: ${t.title}${t.dependsOn.length ? ` (after ${t.dependsOn.join(', ')})` : ''}`));
    el('req-body').replaceChildren(...parts);
  } catch (err) {
    showError(err.message);
  }
}

async function build() {
  showError('');
  el('busy').textContent = 'building…';
  el('build').disabled = true;
  try {
    const project = await call('create_project', { workspace: state.workspace, prompt: el('prompt').value });
    state.projectId = project.id;
    state.projectDir = project.rootPath || null;
    const result = await call('build_project', {
      workspace: state.workspace,
      projectId: project.id,
      prompt: el('prompt').value,
      useLocalModel: Boolean(state.runtime),
    });
    renderReport(result.report);
    el('export').disabled = false;
    el('checkpoint').disabled = false;
    el('rescan').disabled = false;
    // Show the tree as it exists on disk, not just the files the report mentioned.
    await loadProjectFiles();
    await loadMemory();
    await loadProjects();
    await loadCheckpoints(project.id);
  } catch (err) {
    showError(`${err.message}${err.kind ? ` (${err.kind})` : ''}`);
  } finally {
    el('busy').textContent = '';
    el('build').disabled = false;
  }
}

function renderReport(report) {
  el('report-card').hidden = false;
  el('report-sub').textContent = `${report.tasksVerified.length} verified · ${report.tasksUnverified.length} unverified · ${(report.durationMs / 1000).toFixed(1)}s`;
  el('stages').replaceChildren(
    ...report.stages.map((stage) => {
      const tr = document.createElement('tr');
      const name = document.createElement('td');
      name.textContent = stage.stage;
      const ok = document.createElement('td');
      const pill = document.createElement('span');
      pill.className = `pill ${stage.ok ? 'ok' : 'bad'}`;
      pill.textContent = stage.ok ? 'ok' : 'failed';
      ok.append(pill);
      const detail = document.createElement('td');
      detail.className = 'muted';
      detail.textContent = stage.detail;
      tr.append(name, ok, detail);
      return tr;
    }),
  );
  const modelFiles = report.modelFiles || [];
  const modelLine = el('model-files');
  if (modelLine) {
    modelLine.textContent = modelFiles.length
      ? `Model-authored code accepted: ${modelFiles.join(', ')}`
      : 'No model-authored code in this build.';
  }

  const security = el('security');
  security.textContent = `${report.securitySummary} — ${report.securityPassed ? 'passed' : 'FAILED'}`;
  security.className = report.securityPassed ? 'muted' : 'error';

  el('limitations').replaceChildren(
    ...report.limitations.map((text) => {
      const li = document.createElement('li');
      li.textContent = text;
      return li;
    }),
  );
  el('files').replaceChildren(
    ...report.files.map((file) => {
      const li = document.createElement('li');
      const button = document.createElement('button');
      button.className = 'link';
      button.textContent = `${file.path}  ·  ${file.origin}`;
      button.addEventListener('click', () => openFile(file.path));
      li.append(button);
      return li;
    }),
  );
}

// Take a checkpoint at a moment of the user's choosing, not only before a build.
async function checkpointNow() {
  if (!state.projectId) return;
  showError('');
  try {
    const label = window.prompt('Label for this checkpoint', 'manual checkpoint');
    if (label === null) return;
    const created = await call('create_checkpoint', {
      workspace: state.workspace,
      projectId: state.projectId,
      label,
    });
    el('busy').textContent = `checkpoint ${created.id} saved`;
    await loadCheckpoints(state.projectId);
  } catch (err) {
    showError(`Checkpoint failed: ${err.message}`);
  }
}

// Re-run the security scan after the user has changed files.
async function rescan() {
  if (!state.projectId) return;
  showError('');
  try {
    const report = await call('security_scan', {
      workspace: state.workspace,
      projectId: state.projectId,
    });
    const security = el('security');
    security.textContent = `${report.summary} — ${report.passed ? 'passed' : 'FAILED'}`;
    security.className = report.passed ? 'muted' : 'error';
    el('busy').textContent = `re-scanned ${report.filesScanned} file(s)`;
    if (!report.passed) {
      showError(report.findings.map((f) => `${f.path}:${f.line} ${f.rule} (${f.severity})`).join('; '));
    }
  } catch (err) {
    showError(`Scan failed: ${err.message}`);
  }
}

// Read one file from the project. The webview has no filesystem permission, so this goes
// through the path-guarded host command.
async function openFile(path) {
  if (!state.projectId) return;
  try {
    const result = await call('read_project_file', {
      root: state.projectDir,
      relPath: path,
    });
    const view = el('file-view');
    view.hidden = false;
    view.textContent = result.content.length > 20000
      ? `${result.content.slice(0, 20000)}\n\n… truncated`
      : result.content;
    el('file-hint').textContent = `Reading ${path}`;
  } catch (err) {
    showError(`Cannot read ${path}: ${err.message}`);
  }
}

// Reopen a project that already exists. Without this the app could only act on the project
// it had just built, so restarting the application lost access to all previous work.
async function selectProject(project) {
  state.projectId = project.id;
  state.projectDir = project.rootPath || null;
  showError('');
  el('busy').textContent = `opened ${project.name}`;
  for (const id of ['export', 'checkpoint', 'rescan']) {
    const button = el(id);
    if (button) button.disabled = false;
  }
  await loadCheckpoints(project.id);
  await loadProjectFiles();
  await loadMemory();
}

// The files panel works without a build: list the tree through the path-guarded command.
async function loadProjectFiles() {
  if (!state.projectId || !state.projectDir) return;
  try {
    const listing = await call('list_files', { root: state.projectDir });
    const card = el('report-card');
    if (card.hidden) card.hidden = false;
    el('files').replaceChildren(
      ...(listing.files || []).slice(0, 200).map((path) => {
        const li = document.createElement('li');
        const button = document.createElement('button');
        button.className = 'link';
        button.textContent = path;
        button.addEventListener('click', () => openFile(path));
        li.append(button);
        return li;
      }),
    );
    el('file-hint').textContent = `${(listing.files || []).length} file(s) — select one to read it.`;
  } catch (err) {
    showError(`Cannot list files: ${err.message}`);
  }
}

async function loadCheckpoints(projectId) {
  try {
    const result = await call('list_checkpoints', { workspace: state.workspace, projectId });
    el('checkpoints').replaceChildren(
      ...result.checkpoints.map((cp) => {
        const li = document.createElement('li');
        li.textContent = `${cp.label} · ${cp.treeHash.slice(0, 12)}… `;
        const button = document.createElement('button');
        button.textContent = 'Restore';
        button.addEventListener('click', async () => {
          button.disabled = true;
          try {
            const restored = await call('restore_checkpoint', {
              workspace: state.workspace,
              projectId,
              checkpointId: cp.id,
            });
            button.textContent = `restored ${restored.restored.length}, removed ${restored.removed.length}`;
          } catch (err) {
            button.textContent = err.message;
          }
        });
        li.append(button);
        return li;
      }),
    );
  } catch {
    /* no project yet */
  }
}

// Local model management.
//
// Kavach never downloads weights: the licence gate and the hash check exist so that a file
// the user already has can be adopted safely. Both are enforced in Rust, not here.
async function loadModels() {
  const disk = el('disk-line');
  if (!disk) return;
  try {
    const usage = await call('disk_usage', { path: state.workspace || '.' });
    disk.textContent =
      'Disk: ' + human(usage.availableBytes) + ' free of ' + human(usage.totalBytes) +
      ' — enough for a model up to ' + human(usage.availableBytes * 0.8) + ' at 80% of free space.';
  } catch (err) {
    disk.textContent = 'Disk space unavailable: ' + err.message;
  }
}

// The catalogue answers "what should I run on this machine", from a live probe.
async function loadCatalog() {
  const budget = el('model-budget');
  if (!budget) return;
  try {
    const catalog = await call('model_catalog', { workspace: state.workspace || '.' });
    budget.textContent =
      'Budget for model weights: ' + human(catalog.budgetBytes) +
      ' (70% of free memory, or GPU memory when there is a real one).';

    const rows = (catalog.recommended || []).map((rec) => {
      const tr = document.createElement('tr');
      const name = document.createElement('td');
      name.textContent = rec.name;
      const quant = document.createElement('td');
      quant.textContent = rec.quant;
      const size = document.createElement('td');
      size.textContent = human(rec.sizeBytes);
      const fit = document.createElement('td');
      const pill = document.createElement('span');
      pill.className = 'pill ' + (rec.verdict === 'comfortable' ? 'ok' : '');
      pill.textContent = rec.verdict;
      fit.append(pill);
      const action = document.createElement('td');
      const use = document.createElement('button');
      use.className = 'link';
      use.textContent = 'Use this file…';
      use.addEventListener('click', () => {
        // Pre-fill the install form with what the recommendation implies.
        el('model-id').value = rec.id + '-' + rec.quant.toLowerCase();
        el('model-source').focus();
        el('install-result').textContent =
          'Point this at your ' + rec.name + ' ' + rec.quant +
          ' file (about ' + human(rec.sizeBytes) + ').';
      });
      action.append(use);
      tr.append(name, quant, size, fit, action);
      return tr;
    });
    document.querySelector('#recommended tbody').replaceChildren(...rows);
  } catch (err) {
    budget.textContent = 'Cannot read the catalogue: ' + err.message;
  }
}

// What is installed, and which one is active.
async function loadInstalledModels() {
  const summary = el('installed-summary');
  if (!summary) return;
  try {
    const listing = await call('models_list', { workspace: state.workspace || '.' });
    const models = listing.models || [];
    summary.textContent = models.length
      ? models.length + ' model(s), ' + human(listing.totalBytes) + ' on disk' +
        (listing.active ? ' · active: ' + listing.active : ' · no model active')
      : 'Nothing installed yet.';

    document.querySelector('#installed tbody').replaceChildren(
      ...models.map((model) => {
        const tr = document.createElement('tr');
        const name = document.createElement('td');
        name.textContent = model.specId + (model.present ? '' : ' (file missing)');
        const quant = document.createElement('td');
        quant.textContent = model.quant;
        const size = document.createElement('td');
        size.textContent = human(model.sizeBytes);
        const active = document.createElement('td');
        const pill = document.createElement('span');
        pill.className = 'pill ' + (model.active ? 'ok' : '');
        pill.textContent = model.active ? 'active' : '—';
        active.append(pill);
        const actions = document.createElement('td');
        if (!model.active) {
          const activate = document.createElement('button');
          activate.className = 'link';
          activate.textContent = 'Activate';
          activate.addEventListener('click', async () => {
            try {
              await call('model_activate', { workspace: state.workspace || '.', id: model.id });
              await loadInstalledModels();
            } catch (err) {
              showError('Cannot activate: ' + err.message);
            }
          });
          actions.append(activate);
        }
        const forget = document.createElement('button');
        forget.className = 'link';
        forget.textContent = 'Forget';
        forget.addEventListener('click', async () => {
          try {
            await call('model_forget', { workspace: state.workspace || '.', id: model.id });
            await loadInstalledModels();
          } catch (err) {
            showError('Cannot forget: ' + err.message);
          }
        });
        actions.append(forget);
        tr.append(name, quant, size, active, actions);
        return tr;
      }),
    );
  } catch (err) {
    summary.textContent = 'Cannot list models: ' + err.message;
  }
}

async function installModel(event) {
  event.preventDefault();
  const form = el('install-form');
  const errorEl = el('install-error');
  const resultEl = el('install-result');
  errorEl.hidden = true;
  resultEl.textContent = '';

  const data = new FormData(form);
  if (!data.get('accepted')) {
    errorEl.hidden = false;
    errorEl.textContent = 'The licence must be accepted before a model is installed.';
    return;
  }

  try {
    const result = await call('install_model', {
      workspace: state.workspace || '.',
      modelId: String(data.get('modelId')),
      sourceFile: String(data.get('source')),
      destinationDir: (state.workspace || '.') + '/models',
      expectedSha256: String(data.get('sha256')),
      licenceAccepted: true,
      licenceName: String(data.get('licenceName')),
    });
    resultEl.textContent =
      'Installed ' + result.modelId + ' (' + human(result.sizeBytes) + ') at ' + result.path +
      ' — sha256 ' + result.sha256.slice(0, 16) + '…';
    form.reset();
    await loadModels();
    await loadCatalog();
    await loadInstalledModels();
  } catch (err) {
    errorEl.hidden = false;
    errorEl.textContent = 'Install refused: ' + err.message;
  }
}

async function verifyModel(event) {
  event.preventDefault();
  const data = new FormData(el('verify-form'));
  const resultEl = el('verify-result');
  const path = String(data.get('path'));
  const expected = String(data.get('expected') || '').trim();
  try {
    // Read the header first: hashing a 4 GB file only to discover it is an HTML error page
    // saved as .gguf wastes the user's time.
    const inspect = await call('model_inspect', { path });
    if (!inspect.looksLikeAModel) {
      resultEl.textContent =
        'Not a model file: ' + (inspect.problem || 'the header was not recognised') +
        ' (' + human(inspect.sizeBytes) + ')';
      return;
    }
    const digest = await call('hash_file', { path });
    if (!expected) {
      resultEl.textContent = 'sha256 ' + digest.sha256;
      return;
    }
    const check = await call('verify_model', { path, expectedSha256: expected });
    resultEl.textContent = check.matches
      ? 'Matches the expected digest.'
      : 'DOES NOT match the expected digest.';
  } catch (err) {
    resultEl.textContent = 'Cannot read that file: ' + err.message;
  }
}

// The ledger is the product's memory: what was done, in what state, on what evidence.
async function loadMemory() {
  const summary = el('memory-summary');
  if (!summary) return;
  if (!state.projectId) {
    summary.textContent = 'Open a project to see its ledger.';
    return;
  }
  try {
    const memory = await call('project_memory', {
      workspace: state.workspace,
      projectId: state.projectId,
    });
    const tasks = memory.tasks || [];
    const notes = memory.notes || [];
    const counts = tasks.reduce((acc, task) => {
      acc[task.state] = (acc[task.state] || 0) + 1;
      return acc;
    }, {});
    const summaryText = Object.entries(counts).map(([state, n]) => n + ' ' + state).join(' · ');
    summary.textContent = tasks.length
      ? tasks.length + ' task(s): ' + summaryText
      : 'No tasks recorded for this project yet.';

    document.querySelector('#tasks tbody').replaceChildren(
      ...tasks.map((task) => {
        const tr = document.createElement('tr');
        const title = document.createElement('td');
        title.textContent = task.title;
        const stateCell = document.createElement('td');
        const pill = document.createElement('span');
        pill.className = 'pill ' + (task.state === 'verified' ? 'ok' : task.state === 'failed' ? 'bad' : '');
        pill.textContent = task.state;
        stateCell.append(pill);
        const evidence = document.createElement('td');
        evidence.className = 'muted';
        evidence.textContent = (task.evidence || []).join('; ') || '—';
        tr.append(title, stateCell, evidence);
        return tr;
      }),
    );

    el('notes').replaceChildren(
      ...notes.map((note) => {
        const li = document.createElement('li');
        li.textContent = note.kind + ': ' + note.text + (note.evidence ? ' (' + note.evidence + ')' : '');
        return li;
      }),
    );
  } catch (err) {
    summary.textContent = 'Cannot read the ledger: ' + err.message;
  }
}

async function loadProjects() {
  try {
    const result = await call('list_projects', { workspace: state.workspace });
    el('projects-empty').hidden = result.projects.length > 0;
    el('projects').replaceChildren(
      ...result.projects.map((project) => {
        const li = document.createElement('li');
        const open = document.createElement('button');
        open.className = 'link';
        open.textContent = `${project.name} · ${project.appKind} · ${project.verifiedTasks} verified / ${project.unverifiedTasks} unverified`;
        open.addEventListener('click', () => selectProject(project));
        li.append(open);
        return li;
      }),
    );
  } catch (err) {
    el('projects-empty').textContent = `cannot list projects: ${err.message}`;
  }
}

async function exportProject() {
  if (!state.projectId) return;
  showError('');
  const destination = window.prompt(
    'Export to which folder? Give an absolute path outside the project.',
    'C:\\Users\\Public\\kavach-export',
  );
  if (!destination) return;
  try {
    const result = await call('export_project', {
      workspace: state.workspace,
      projectId: state.projectId,
      destination,
    });
    el('busy').textContent = `exported ${result.files} files (${result.bytes} bytes) to ${result.destination}`;
  } catch (err) {
    showError(`Export failed: ${err.message}`);
  }
}

el('build').addEventListener('click', build);
el('export').addEventListener('click', exportProject);
el('preview-req').addEventListener('click', showRequirements);
el('checkpoint').addEventListener('click', checkpointNow);
el('rescan').addEventListener('click', rescan);
el('install-form').addEventListener('submit', installModel);
el('verify-form').addEventListener('submit', verifyModel);
boot();
