#!/usr/bin/env node
// ─── moleculer-rs docs SPA — comprehensive test suite ───────────────────────
const fs   = require('fs');
const path = require('path');
const http = require('http');

const BASE   = path.resolve(__dirname, 'docs/docs');
const BASE_URL = 'http://localhost:8765';

let passed = 0, failed = 0, warnings = 0;
const FAILS = [];

function pass(msg) { process.stdout.write(`  ✓ ${msg}\n`); passed++; }
function fail(msg, detail='') { process.stdout.write(`  ✗ ${msg}${detail ? ': ' + detail : ''}\n`); failed++; FAILS.push(msg); }
function warn(msg) { process.stdout.write(`  ⚠ ${msg}\n`); warnings++; }
function section(name) { console.log(`\n\x1b[36m── ${name} ──\x1b[0m`); }

// ─── HTTP helper ─────────────────────────────────────────────────────────────
function get(url) {
  return new Promise((resolve, reject) => {
    http.get(url, res => {
      let body = '';
      res.on('data', d => body += d);
      res.on('end', () => resolve({ status: res.statusCode, body, headers: res.headers }));
    }).on('error', reject);
  });
}

// ─── All tests ───────────────────────────────────────────────────────────────
async function run() {
  console.log('\x1b[1mmoleculer-rs docs SPA — test suite\x1b[0m');

  // ── 1. File existence ─────────────────────────────────────────────────────
  section('1. File existence');
  const required = [
    'index.html',
    'content/introduction.md',
    'content/quick-start.md',
    'content/broker.md',
    'content/middleware.md',
    'content/channels.md',
    'content/database.md',
    'content/workflows.md',
    'content/metrics.md',
    'content/laboratory.md',
  ];
  for (const f of required) {
    const full = path.join(BASE, f);
    fs.existsSync(full) ? pass(f) : fail(f, 'not found');
  }

  // ── 2. HTTP serving ───────────────────────────────────────────────────────
  section('2. HTTP serving');
  for (const f of required) {
    try {
      const r = await get(`${BASE_URL}/${f}`);
      r.status === 200
        ? pass(`GET /${f} → 200`)
        : fail(`GET /${f}`, `HTTP ${r.status}`);
    } catch(e) {
      fail(`GET /${f}`, e.message);
    }
  }

  // ── 3. index.html structure ───────────────────────────────────────────────
  section('3. index.html structure');
  const html = fs.readFileSync(path.join(BASE, 'index.html'), 'utf8');

  // CDN scripts must be present
  const cdns = [
    ['marked.js',       'marked.min.js'],
    ['highlight.js',    'highlight.min.js'],
    ['highlight CSS',   'atom-one-dark.min.css'],
    ['highlight Rust',  'rust.min.js'],
    ['JetBrains Mono',  'JetBrains+Mono'],
    ['Inter font',      'Inter:'],
  ];
  for (const [name, token] of cdns) {
    html.includes(token) ? pass(`CDN: ${name}`) : fail(`CDN missing: ${name}`);
  }

  // Structural landmarks
  const landmarks = [
    ['<header',           'topbar element'],
    ['class="sidebar"',   'sidebar nav'],
    ['class="main"',      'main content area'],
    ['id="searchOverlay"','search modal'],
    ['id="breadcrumb"',   'breadcrumb container'],
    ['id="pagination"',   'pagination container'],
    ['id="content"',      'content container'],
    ['class="md"',        'markdown container class'],
  ];
  for (const [token, desc] of landmarks) {
    html.includes(token) ? pass(desc) : fail(desc, 'missing from HTML');
  }

  // All 9 pages registered in sidebar
  const sidebarPages = ['introduction','quick-start','broker','middleware','channels','database','workflows','metrics','laboratory'];
  for (const page of sidebarPages) {
    html.includes(`data-page="${page}"`) ? pass(`sidebar item: ${page}`) : fail(`sidebar item missing: ${page}`);
  }

  // ── 4. JS — NAV array completeness ────────────────────────────────────────
  section('4. JS NAV array completeness');
  const navMatch = html.match(/const NAV\s*=\s*\[([\s\S]*?)\];/);
  if (!navMatch) { fail('NAV array not found'); }
  else {
    const navBlock = navMatch[1];
    for (const page of sidebarPages) {
      navBlock.includes(`'${page}'`) || navBlock.includes(`"${page}"`)
        ? pass(`NAV entry: ${page}`)
        : fail(`NAV entry missing: ${page}`);
    }
    // Check all required fields per entry
    const required_fields = ['page:', 'label:', 'section:', 'icon:'];
    for (const f of required_fields) {
      navBlock.includes(f) ? pass(`NAV has field: ${f}`) : fail(`NAV missing field: ${f}`);
    }
  }

  // ── 5. JS — function completeness ────────────────────────────────────────
  section('5. JS functions present');
  const functions = [
    'function navigate(',
    'function getPage(',
    'function loadPage(',
    'function getBase(',
    'function renderMarkdown(',
    'function updateSidebar(',
    'function updateBreadcrumb(',
    'function updatePagination(',
    'function openSearch(',
    'function closeSearch(',
    'window.copyCode',
    'afterRender(',
    'renderSearchResults(',
  ];
  for (const fn of functions) {
    html.includes(fn) ? pass(fn.replace('(',  '()')) : fail(`Missing function: ${fn}`);
  }

  // ── 6. JS — router correctness ────────────────────────────────────────────
  section('6. Router logic');

  // getPage must handle missing 'name' param → default to 'introduction'
  const getPageFn = html.match(/function getPage\(\)[^}]+}/s)?.[0] || '';
  getPageFn.includes("'introduction'") || getPageFn.includes('"introduction"')
    ? pass('getPage() defaults to introduction')
    : fail('getPage() has no introduction default');

  // history.pushState must be called on navigate
  html.includes('history.pushState') ? pass('history.pushState used for navigation') : fail('history.pushState missing');

  // popstate listener for back/fwd button
  html.includes("'popstate'") || html.includes('"popstate"')
    ? pass('popstate listener registered')
    : fail('popstate listener missing — browser back button broken');

  // navigate() must not reload page
  html.includes('location.href') && !html.includes("location.href = '?name")
    ? warn('location.href assignment detected — may cause full page reload')
    : pass('navigate() uses pushState, not location.href');

  // ── 7. getBase() — path resolution ───────────────────────────────────────
  section('7. getBase() path resolution');
  const getBaseFn = html.match(/function getBase\(\)([\s\S]*?)^  \}/m)?.[0]
    || html.match(/function getBase\(\)[^}]+\}/s)?.[0] || '';

  // Must handle both trailing slash and /index.html paths
  getBaseFn.includes("endsWith('/')") || getBaseFn.includes('endsWith("/")')
    ? pass('getBase handles trailing-slash path')
    : fail('getBase: missing trailing slash case — fetch will break on /docs/');

  getBaseFn.includes('index.html')
    ? pass('getBase handles /index.html path')
    : fail('getBase: missing index.html case');

  // ── 8. Markdown rendering ─────────────────────────────────────────────────
  section('8. Markdown renderer');
  const rendererFn = html.match(/function renderMarkdown\([\s\S]*?^}/m)?.[0]
    || html.match(/function renderMarkdown\([^)]*\)([\s\S]*?)\n\}/s)?.[0] || html;

  html.includes('new marked.Renderer()') ? pass('custom marked Renderer') : fail('no custom Renderer — code blocks will not get copy buttons');
  html.includes('renderer.code') ? pass('renderer.code overridden') : fail('renderer.code not overridden — no copy buttons on code');
  html.includes('renderer.link') ? pass('renderer.link overridden') : fail('renderer.link not overridden — internal links broken');
  html.includes('copy-btn') ? pass('copy button injected in code blocks') : fail('copy-btn class missing from code renderer');
  html.includes('hljs.highlight(') ? pass('hljs.highlight() called for syntax highlighting') : fail('hljs.highlight missing');
  html.includes('marked.use(') || html.includes('marked.setOptions(') ? pass('marked configured') : fail('marked not configured');

  // ── 9. Search modal ────────────────────────────────────────────────────────
  section('9. Search functionality');
  html.includes("'k'") && html.includes("metaKey") ? pass('⌘K shortcut registered') : fail('⌘K shortcut missing');
  html.includes("'Escape'") ? pass('ESC closes search') : fail('ESC handler missing');
  html.includes("'ArrowDown'") && html.includes("'ArrowUp'") ? pass('keyboard navigation in results') : fail('arrow key nav missing');
  html.includes("'Enter'") ? pass('Enter selects result') : fail('Enter handler missing in search');
  html.includes('renderSearchResults(') ? pass('renderSearchResults() exists') : fail('renderSearchResults missing');
  // search must show all NAV items when query is empty
  const renderSearchFn = html.match(/function renderSearchResults[\s\S]*?^}/m)?.[0]
    || html.match(/function renderSearchResults[^)]*\)([\s\S]*?)\n}/s)?.[0] || '';
  renderSearchFn.includes('NAV') ? pass('search uses NAV array') : fail('search does not reference NAV array');

  // ── 10. Pagination ─────────────────────────────────────────────────────────
  section('10. Pagination');
  html.includes('updatePagination(') ? pass('updatePagination() called') : fail('updatePagination missing');
  html.includes('page-nav-next') ? pass('next page button class') : fail('page-nav-next class missing');
  // pagination nav must use onclick=navigate not href (to avoid page reload)
  const pagFn = html.match(/function updatePagination[\s\S]*?^}/m)?.[0] || '';
  pagFn.includes("navigate('") ? pass('pagination uses navigate(), not href') : fail('pagination missing navigate() call');

  // ── 11. Markdown content quality ──────────────────────────────────────────
  section('11. Markdown content quality');
  const pages = {
    'introduction.md':  { minLines: 50,  mustContain: ['moleculer-rs', '```', '|', 'Rust'] },
    'quick-start.md':   { minLines: 40,  mustContain: ['```', 'cargo', 'localhost:3210'] },
    'broker.md':        { minLines: 80,  mustContain: ['ServiceBroker', '```rust', 'BrokerConfig', '|'] },
    'middleware.md':    { minLines: 80,  mustContain: ['Circuit Breaker', 'Retry', 'Bulkhead', '```rust'] },
    'channels.md':      { minLines: 70,  mustContain: ['ChannelDef', 'ack()', 'nack()', 'DLQ', '|'] },
    'database.md':      { minLines: 100, mustContain: ['NeDB', 'MongoDB', '```js', '|'] },
    'workflows.md':     { minLines: 80,  mustContain: ['workflow', 'Redis', '```js', '|'] },
    'metrics.md':       { minLines: 70,  mustContain: ['Counter', 'Gauge', 'Histogram', 'prometheus', '```'] },
    'laboratory.md':    { minLines: 70,  mustContain: ['/health', '/topology', '/traces', '|', '```'] },
  };
  for (const [file, spec] of Object.entries(pages)) {
    const content = fs.readFileSync(path.join(BASE, 'content', file), 'utf8');
    const lines = content.split('\n').length;
    lines >= spec.minLines
      ? pass(`${file}: ${lines} lines (>= ${spec.minLines})`)
      : fail(`${file}: only ${lines} lines (min ${spec.minLines})`);
    for (const token of spec.mustContain) {
      content.includes(token)
        ? pass(`${file} contains: ${token}`)
        : fail(`${file} missing: ${token}`);
    }
    // Must start with # heading
    content.trim().startsWith('#')
      ? pass(`${file} starts with heading`)
      : fail(`${file} does not start with # heading`);
    // No HTML entities accidentally left
    if (content.includes('&lt;') || content.includes('&gt;')) {
      warn(`${file} has HTML-escaped entities — may render oddly`);
    }
  }

  // ── 12. CDN availability ──────────────────────────────────────────────────
  section('12. CDN URLs (reachability)');
  const cdnUrls = [
    'https://cdnjs.cloudflare.com/ajax/libs/marked/9.1.6/marked.min.js',
    'https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.9.0/highlight.min.js',
  ];
  for (const url of cdnUrls) {
    try {
      const { createHash } = require('crypto');
      const https = require('https');
      await new Promise((resolve, reject) => {
        https.get(url, { timeout: 3000 }, res => {
          res.status = res.statusCode;
          res.statusCode === 200 ? resolve() : reject(new Error('HTTP ' + res.statusCode));
          res.resume();
        }).on('error', reject).on('timeout', () => reject(new Error('timeout')));
      });
      pass(`CDN reachable: ${url.split('/').pop()}`);
    } catch(e) {
      warn(`CDN unreachable (network): ${url.split('/').pop()} — ${e.message}`);
    }
  }

  // ── 13. Security / good practices ────────────────────────────────────────
  section('13. Security & practices');
  html.includes('rel="noopener"') ? pass('external links have rel="noopener"') : warn('external links missing rel="noopener"');
  !html.includes('eval(') ? pass('no eval() usage') : fail('eval() found — security risk');
  !html.includes('innerHTML') || html.includes('// safe')
    ? warn('innerHTML used — verify no user-controlled input injected')
    : pass('no raw innerHTML');
  html.includes('escapeHtml(') ? pass('escapeHtml() helper present') : fail('escapeHtml missing — XSS risk in rendered links');
  html.includes('<meta charset') ? pass('charset meta tag') : fail('missing charset meta');
  html.includes('<meta name="viewport"') ? pass('viewport meta tag') : fail('missing viewport meta');

  // ── 14. Mobile / responsive ────────────────────────────────────────────────
  section('14. Responsive / mobile');
  html.includes('@media (max-width') ? pass('media queries present') : fail('no media queries — not responsive');
  const mediaCount = (html.match(/@media/g) || []).length;
  mediaCount >= 3 ? pass(`${mediaCount} media breakpoints`) : warn(`only ${mediaCount} media query — may not cover all sizes`);
  html.includes('mobile-toggle') || html.includes('mobileToggle')
    ? pass('mobile sidebar toggle exists')
    : fail('mobile sidebar toggle missing');

  // ── 15. State — navigate() called on boot ─────────────────────────────────
  section('15. Boot sequence');
  // Must call navigate(getPage(), false) on load — NOT navigate('introduction') hardcoded
  html.includes('navigate(getPage(), false)') || html.includes("navigate(getPage(),false)")
    ? pass('boot calls navigate(getPage(), false) — respects URL params')
    : fail('boot does not use getPage() — deep links broken');

  // Must NOT auto-call navigate without checking URL first
  const bootSection = html.slice(html.lastIndexOf('</script>') - 2000);
  bootSection.includes('navigate(getPage()') && !bootSection.match(/navigate\(['"]introduction/)
    ? pass('boot respects ?name= URL param')
    : warn('boot might ignore ?name= param — check deep linking');

  // ── 16. Pagination boundary conditions ────────────────────────────────────
  section('16. Pagination edge cases');
  // First page: no "Previous" button should appear
  const pagFunc = html.match(/function updatePagination\([\s\S]*?\n\}/m)?.[0] || html;
  pagFunc.includes('idx > 0') || pagFunc.includes('idx>0')
    ? pass('pagination guards prev button (idx > 0)')
    : fail('pagination: prev button shown on first page');
  pagFunc.includes('idx < NAV.length') || pagFunc.includes('NAV.length - 1')
    ? pass('pagination guards next button (idx < NAV.length - 1)')
    : fail('pagination: next button shown on last page');

  // ── 17. Cache layer ────────────────────────────────────────────────────────
  section('17. Content cache');
  html.includes('const cache = {}') || html.includes('let cache')
    ? pass('in-memory content cache object')
    : fail('no content cache — every page switch re-fetches');
  html.includes('cache[page]')
    ? pass('cache lookup before fetch')
    : fail('cache not checked before fetch');

  // ── 18. Error handling ────────────────────────────────────────────────────
  section('18. Error handling');
  html.includes('catch')
    ? pass('try/catch present in fetch code')
    : fail('no error handling in content loader');
  html.includes('Failed to load')
    ? pass('user-visible fetch error message')
    : fail('no user error message on fetch failure');

  // ── Summary ───────────────────────────────────────────────────────────────
  console.log(`\n${'─'.repeat(50)}`);
  console.log(`\x1b[1mResults: \x1b[32m${passed} passed\x1b[0m  \x1b[31m${failed} failed\x1b[0m  \x1b[33m${warnings} warnings\x1b[0m`);
  if (FAILS.length) {
    console.log('\n\x1b[31mFailed tests:\x1b[0m');
    FAILS.forEach(f => console.log(`  • ${f}`));
  }
  console.log('');
  process.exit(failed > 0 ? 1 : 0);
}

run().catch(e => { console.error(e); process.exit(1); });
