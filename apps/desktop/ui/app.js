// Kavach desktop UI.
//
// Plain JavaScript, no bundler: the app is a static page and every privileged action is a
// Tauri command. `invoke` returns the command's JSON envelope, so errors are rendered
// rather than thrown into the console.

const invoke = window.__TAURI__?.invoke;

const el = (id) => document.getElementById(id);
const state = { workspace: null, projectId: null };

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
    const result = await call('build_project', {
      workspace: state.workspace,
      projectId: project.id,
      prompt: el('prompt').value,
    });
    renderReport(result.report);
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
      li.textContent = `${file.path}  ·  ${file.origin}`;
      return li;
    }),
  );
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

async function loadProjects() {
  try {
    const result = await call('list_projects', { workspace: state.workspace });
    el('projects-empty').hidden = result.projects.length > 0;
    el('projects').replaceChildren(
      ...result.projects.map((project) => {
        const li = document.createElement('li');
        li.textContent = `${project.name} · ${project.appKind} · ${project.verifiedTasks} verified / ${project.unverifiedTasks} unverified`;
        return li;
      }),
    );
  } catch (err) {
    el('projects-empty').textContent = `cannot list projects: ${err.message}`;
  }
}

el('build').addEventListener('click', build);
el('preview-req').addEventListener('click', showRequirements);
boot();
