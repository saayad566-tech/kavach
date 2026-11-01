const el = (id) => document.getElementById(id);
const pct = (v) => (v === null || v === undefined ? '—' : (v * 100).toFixed(2) + '%');
const num = (v, d = 2) => (v === null || v === undefined ? '—' : Number(v).toFixed(d));

async function api(path) {
  const res = await fetch(path);
  const body = await res.json().catch(() => ({}));
  if (!res.ok) throw new Error(body.error || 'Request failed with ' + res.status);
  return body;
}

function showError(message) {
  const box = el('error');
  box.hidden = !message;
  box.textContent = message || '';
}

function fillTable(id, rows) {
  const tbody = document.querySelector('#' + id + ' tbody');
  tbody.replaceChildren(...rows.map(([label, value]) => {
    const tr = document.createElement('tr');
    const th = document.createElement('th');
    th.scope = 'row';
    th.textContent = label;
    const td = document.createElement('td');
    td.textContent = value === null || value === undefined ? '—' : String(value);
    tr.append(th, td);
    return tr;
  }));
}

async function runScreen(event) {
  if (event) event.preventDefault();
  showError('');
  try {
    const metric = el('screen-metric').value;
    const op = el('screen-op').value;
    const value = el('screen-value').value;
    const result = await api(`/api/screen?metric=${metric}&op=${op}&value=${value}`);
    el('screen-summary').textContent =
      `${result.passed.length} of ${result.total} pass · ${result.failed.length} fail · ` +
      `${result.excluded.length} excluded for missing ${metric}`;
    el('screen-results').replaceChildren(...result.passed.map((symbol) => {
      const li = document.createElement('li');
      li.textContent = symbol;
      return li;
    }));
  } catch (err) {
    showError('Screen failed: ' + err.message);
  }
}

async function loadRanking() {
  try {
    const metric = el('rank-metric').value;
    const direction = metric === 'peRatio' ? 'asc' : 'desc';
    const result = await api(`/api/rank?metric=${metric}&dir=${direction}`);
    document.querySelector('#rank-table tbody').replaceChildren(...result.rows.map((row) => {
      const tr = document.createElement('tr');
      for (const text of [row.symbol, row.sector, row.value === null ? '—' : num(row.value, 3)]) {
        const td = document.createElement('td');
        td.textContent = text;
        tr.append(td);
      }
      return tr;
    }));
  } catch (err) {
    showError('Ranking failed: ' + err.message);
  }
}

async function loadSectors() {
  try {
    const result = await api('/api/sectors?metric=roe');
    el('sectors').replaceChildren(...result.sectors.map((sector) => {
      const block = document.createElement('div');
      block.className = 'sector';
      const heading = document.createElement('h3');
      heading.textContent = `${sector.sector} — median ROE ${pct(sector.median)}`;
      const list = document.createElement('ul');
      list.className = 'tight';
      for (const member of sector.members) {
        const li = document.createElement('li');
        li.textContent = `${member.symbol}: ${pct(member.value)}` +
          (member.percentile === null ? '' : ` (${Math.round(member.percentile * 100)}th pct)`);
        list.append(li);
      }
      block.append(heading, list);
      return block;
    }));
  } catch (err) {
    showError('Sectors failed: ' + err.message);
  }
}

async function loadCompany() {
  const symbol = el('company-select').value;
  if (!symbol) return;
  showError('');
  try {
    const company = await api('/api/company/' + encodeURIComponent(symbol));
    const a = company.analysis;
    el('company-summary').textContent =
      `${company.name} · ${company.sector} · period ${company.period}` +
      (company.priorPeriod ? ` vs ${company.priorPeriod}` : ' (no prior period)') +
      (a.flags.length ? ` · ${a.flags.length} flag(s)` : ' · no flags');

    fillTable('c-profitability', [
      ['gross margin', pct(a.profitability.grossMargin)],
      ['operating margin', pct(a.profitability.operatingMargin)],
      ['net margin', pct(a.profitability.netMargin)],
      ['ROE', pct(a.profitability.roe)],
      ['ROIC', pct(a.profitability.roic)],
    ]);
    fillTable('c-leverage', [
      ['current ratio', num(a.liquidity.currentRatio)],
      ['quick ratio', num(a.liquidity.quickRatio)],
      ['debt / equity', num(a.leverage.debtToEquity)],
      ['interest cover', num(a.leverage.interestCoverage, 1)],
    ]);
    fillTable('c-valuation', [
      ['P/E', num(a.perShare.peRatio)],
      ['P/B', num(a.perShare.pbRatio)],
      ['EV / EBITDA', num(a.valuation.evEbitda)],
      ['FCF margin', pct(a.valuation.fcfMargin)],
    ]);
    fillTable('c-quality', [
      ['Piotroski F', `${a.composites.piotroskiF.score} / ${a.composites.piotroskiF.of}`],
      ['Altman Z', a.composites.altmanZ ? `${a.composites.altmanZ.zone} (${num(a.composites.altmanZ.z)})` : '—'],
      ['DuPont ROE', pct(a.composites.dupont ? a.composites.dupont.roe : null)],
      ['flags', a.flags.length ? a.flags.map((f) => f.flag).join(', ') : 'none'],
    ]);
  } catch (err) {
    showError('Company failed: ' + err.message);
  }
}

async function runDcf(event) {
  if (event) event.preventDefault();
  const symbol = el('company-select').value;
  if (!symbol) return;
  showError('');
  try {
    const q = `growth=${el('dcf-growth').value}&discount=${el('dcf-discount').value}&terminal=${el('dcf-terminal').value}`;
    const result = await api(`/api/dcf/${encodeURIComponent(symbol)}?${q}`);
    const perShare = result.result.perShare;
    el('dcf-summary').textContent =
      `${result.symbol}: ${num(perShare)} per share against a price of ${num(result.price)}` +
      (result.marginOfSafety === null ? '' : ` · margin of safety ${pct(result.marginOfSafety)}`) +
      ` · terminal value is ${pct(result.result.terminalShareOfValue)} of the total`;

    const grid = result.sensitivity;
    const rows = [];
    const header = document.createElement('tr');
    for (const text of ['discount \\ growth', ...grid.growths.map((g) => pct(g))]) {
      const th = document.createElement('th');
      th.textContent = text;
      header.append(th);
    }
    rows.push(header);
    grid.rows.forEach((row, index) => {
      const tr = document.createElement('tr');
      const th = document.createElement('th');
      th.scope = 'row';
      th.textContent = pct(row.discount);
      tr.append(th);
      for (const cell of row.cells) {
        const td = document.createElement('td');
        td.textContent = cell === null ? '—' : num(cell);
        tr.append(td);
      }
      rows.push(tr);
    });
    document.querySelector('#dcf-grid tbody').replaceChildren(...rows);
  } catch (err) {
    showError('Valuation failed: ' + err.message);
  }
}

async function boot() {
  try {
    const health = await api('/api/health');
    el('status').textContent = `${health.companies} companies loaded from data/statements.csv`;

    const list = await api('/api/companies');
    const select = el('company-select');
    for (const company of list.companies) {
      const option = document.createElement('option');
      option.value = company.symbol;
      option.textContent = `${company.symbol} — ${company.name}`;
      select.append(option);
    }
    select.addEventListener('change', () => {
      loadCompany();
      runDcf();
    });

    el('screen-form').addEventListener('submit', runScreen);
    el('rank-metric').addEventListener('change', loadRanking);
    el('dcf-form').addEventListener('submit', runDcf);

    await runScreen();
    await loadRanking();
    await loadSectors();
    await loadCompany();
    await runDcf();
  } catch (err) {
    el('status').textContent = 'Cannot reach the API: ' + err.message;
    showError(err.message);
  }
}

boot();
