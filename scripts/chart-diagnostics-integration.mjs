// Exercise the real compiled app and smoke harness, never a synthetic page.
import assert from 'node:assert/strict';
import http from 'node:http';
import { spawn } from 'node:child_process';
import { mkdir, mkdtemp, readdir, readFile, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { inflateSync } from 'node:zlib';

const env = process.env;
assert.equal(env.ATP_SMOKE_ALLOW_FIXTURES, '1', 'Set ATP_SMOKE_ALLOW_FIXTURES=1');
assert.match(env.ATP_CHART_TEST_DATABASE_URL || '', /^postgresql:\/\/\/atp_chart_[a-z0-9_]+$/,
  'Use a disposable postgresql:///atp_chart_* database');
assert.match(env.ATP_CHART_BASE_URL || '', /^http:\/\/(?:localhost|127\.0\.0\.1|\[::1\])(?::[0-9]+)?\/?$/,
  'Set an explicit HTTP loopback ATP_CHART_BASE_URL without credentials, path, query or hash');
const upstreamURL = new URL(env.ATP_CHART_BASE_URL);
const artifactRoot = env.ATP_CHART_ARTIFACT_DIR || env.JCODE_SCRATCH_DIR;
assert.ok(artifactRoot?.trim(), 'Set ATP_CHART_ARTIFACT_DIR or JCODE_SCRATCH_DIR');
await mkdir(resolve(artifactRoot), { recursive: true });
const output = await mkdtemp(join(resolve(artifactRoot), 'chart-diagnostics-integration-'));
const smoke = fileURLToPath(new URL('./chart-smoke.mjs', import.meta.url));
const finalPass = 'PASS mobile, reduced motion, empty results, no hydration exceptions';
const logLimit = 2 * 1024 * 1024;
let activeChild;
let interrupted = false;

function killChild(signal) {
  if (!activeChild?.pid) return;
  try {
    if (process.platform === 'win32') activeChild.kill(signal);
    else process.kill(-activeChild.pid, signal);
  } catch (error) { if (error.code !== 'ESRCH') throw error; }
}
const interrupt = () => { interrupted = true; killChild('SIGTERM'); };
process.on('SIGINT', interrupt);
process.on('SIGTERM', interrupt);

async function runSmoke(name, base) {
  assert.ok(!interrupted, 'Integration interrupted');
  const directory = await mkdtemp(join(output, `${name}-`));
  let log = Buffer.alloc(0);
  let truncated = false;
  let failure;
  let killTimer;
  const capture = chunk => {
    log = Buffer.concat([log, chunk]);
    if (log.length > logLimit) { log = log.subarray(-logLimit); truncated = true; }
  };
  activeChild = spawn(process.execPath, [smoke], {
    env: { ...env, ATP_CHART_BROWSER: 'chromium', ATP_CHART_BASE_URL: base,
      ATP_CHART_ARTIFACT_DIR: directory, JCODE_SCRATCH_DIR: directory },
    detached: process.platform !== 'win32',
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  const child = activeChild;
  const stop = () => {
    killChild('SIGTERM');
    killTimer ??= setTimeout(() => killChild('SIGKILL'), 2000);
  };
  // Signals also get an escalation deadline if a browser or child ignores TERM.
  process.on('SIGINT', stop);
  process.on('SIGTERM', stop);
  child.stdout.on('data', capture);
  child.stderr.on('data', capture);
  child.on('error', error => { failure = error; });
  const timer = setTimeout(() => { failure = new Error(`${name} smoke timed out`); stop(); }, 120000);
  let result;
  try {
    result = await new Promise(resolve => child.once('close', (code, signal) => resolve({ code, signal })));
  } finally {
    clearTimeout(timer);
    clearTimeout(killTimer);
    // Reap any browser descendants even if the smoke process exited early.
    killChild('SIGKILL');
    activeChild = undefined;
    process.off('SIGINT', stop);
    process.off('SIGTERM', stop);
    await writeFile(join(directory, 'run.log'), Buffer.concat([
      Buffer.from(truncated ? '[Earlier output truncated at 2 MiB]\n' : ''), log,
    ]), { mode: 0o600 });
  }
  if (failure) throw failure;
  assert.ok(!interrupted && !result.signal, `${name} interrupted`);
  return { ...result, directory, log: log.toString('utf8') };
}

const sabotage = `
for (let i = 0; i < 310; i++) console.log('diagnostic noise before hydration', i);
addEventListener('error', () => {
  for (let i = 0; i < 310; i++) console.log('diagnostic noise after error', i);
}, { once: true });
{
  const corrupt = () => {
    const el = document.querySelector('.result-head .provider');
    if (!el) return false;
    const node = document.createElement('b');
    node.textContent = el.textContent;
    el.replaceChildren(node);
    return true;
  };
  if (!corrupt()) {
    const observer = new MutationObserver(() => { if (corrupt()) observer.disconnect(); });
    observer.observe(document.documentElement, { childList: true, subtree: true });
  }
}
`;

// Validate chunk framing, checksums, dimensions and the compressed pixel stream.
function validatePNG(bytes) {
  assert.equal(bytes.subarray(0, 8).toString('hex'), '89504e470d0a1a0a');
  let offset = 8;
  const pixels = [];
  let ended = false;
  while (offset < bytes.length) {
    assert.ok(offset + 12 <= bytes.length, 'Truncated PNG chunk');
    const size = bytes.readUInt32BE(offset);
    const end = offset + 12 + size;
    assert.ok(end <= bytes.length, 'Truncated PNG data');
    const type = bytes.toString('ascii', offset + 4, offset + 8);
    let crc = 0xffffffff;
    for (const byte of bytes.subarray(offset + 4, end - 4)) {
      crc ^= byte;
      for (let bit = 0; bit < 8; bit++) crc = (crc >>> 1) ^ ((crc & 1) ? 0xedb88320 : 0);
    }
    assert.equal((crc ^ 0xffffffff) >>> 0, bytes.readUInt32BE(end - 4), 'PNG CRC');
    if (offset === 8) {
      assert.equal(type, 'IHDR');
      assert.equal(size, 13);
      assert.ok(bytes.readUInt32BE(offset + 8) > 0 && bytes.readUInt32BE(offset + 12) > 0);
    }
    if (type === 'IDAT') pixels.push(bytes.subarray(offset + 8, end - 4));
    offset = end;
    if (type === 'IEND') { assert.equal(size, 0); ended = true; break; }
  }
  assert.ok(ended && offset === bytes.length && pixels.length > 0, 'Incomplete PNG');
  assert.ok(inflateSync(Buffer.concat(pixels), { maxOutputLength: 128 * 1024 * 1024 }).length > 0);
}

let server;
const requests = new Set();
const proxyErrors = [];
let injected = 0;
try {
  const baseline = await runSmoke('baseline', upstreamURL.origin);
  assert.equal(baseline.code, 0, `Baseline failed: see ${baseline.directory}/run.log`);
  assert.ok(baseline.log.split(/\r?\n/).includes(finalPass), 'Missing final baseline PASS');
  assert.ok(!interrupted, 'Integration interrupted');
  server = http.createServer((request, response) => {
    response.on('error', () => {});
    const upstream = http.request({
      hostname: upstreamURL.hostname.replace(/^\[|\]$/g, ''),
      port: upstreamURL.port || 80, method: request.method, path: request.url,
      headers: { ...request.headers, host: upstreamURL.host, 'accept-encoding': 'identity' },
    }, incoming => {
      incoming.on('error', error => response.destroy(error));
      if (request.url.split('?')[0] !== '/pkg/atp.js') {
        response.writeHead(incoming.statusCode, incoming.headers);
        incoming.pipe(response);
        return;
      }
      const chunks = [];
      let size = 0;
      incoming.on('data', chunk => {
        size += chunk.length;
        if (size > 32 * 1024 * 1024) upstream.destroy(new Error('atp.js exceeds 32 MiB'));
        else chunks.push(chunk);
      });
      incoming.on('end', () => {
        if (incoming.statusCode !== 200 || (incoming.headers['content-encoding'] && incoming.headers['content-encoding'] !== 'identity')) {
          upstream.destroy(new Error('Expected uncompressed successful atp.js response'));
          return;
        }
        injected++;
        const headers = { ...incoming.headers };
        for (const key of ['content-length', 'content-encoding', 'etag', 'last-modified']) delete headers[key];
        response.writeHead(incoming.statusCode, headers);
        response.end(Buffer.concat([Buffer.from(sabotage), ...chunks]));
      });
    });
    requests.add(upstream);
    upstream.on('close', () => requests.delete(upstream));
    upstream.on('error', error => {
      if (proxyErrors.length < 10) proxyErrors.push(error.message);
      response.destroy();
    });
    upstream.setTimeout(30000, () => upstream.destroy(new Error('Upstream timed out')));
    request.on('error', () => upstream.destroy());
    response.on('close', () => { if (!response.writableFinished) upstream.destroy(); });
    request.pipe(upstream);
  });
  await new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolve);
  });
  const fault = await runSmoke('hydration-fault', `http://127.0.0.1:${server.address().port}`);
  assert.equal(fault.code, 1, `Expected hydration failure: see ${fault.directory}/run.log`);
  assert.deepEqual(proxyErrors, [], 'Proxy errors');
  assert.ok(injected > 0, 'Real app JS was not injected');
  const directories = (await readdir(fault.directory)).filter(name => name.startsWith('chart-smoke-failure-'));
  assert.equal(directories.length, 1, 'Expected one failure artifact directory');
  const artifacts = join(fault.directory, directories[0]);
  const diagnostics = JSON.parse(await readFile(join(artifacts, 'console.json'), 'utf8'));
  assert.ok(Array.isArray(diagnostics) && diagnostics.length <= 300, 'Bounded diagnostic buffer');
  assert.ok(diagnostics.some(entry => entry.type === 'failure'), 'Failure retained');
  assert.ok(diagnostics.some(entry => entry.type === 'log' && entry.text?.startsWith('diagnostic noise ')),
    'Ordinary console flood reached the diagnostic buffer');
  const pageerror = diagnostics.find(entry => entry.type === 'pageerror' && entry.message?.split('\n').length > 15);
  assert.ok(pageerror, 'Pageerror with more than 15 stack lines retained');
  assert.match(JSON.stringify(diagnostics), /failed_to_cast_text_node/);
  const nodes = diagnostics.flatMap(entry => entry.arguments || []).flatMap(argument => {
    try { const value = JSON.parse(argument); return value?.nodeType ? [value] : []; }
    catch { return []; }
  });
  assert.ok(nodes.some(node => node.nodeType === 1 && /<b[ >]/.test(node.outerHTML || '')), 'Corrupted DOM node arguments retained');
  const html = await readFile(join(artifacts, 'page.html'), 'utf8');
  assert.match(html, /<!doctype html>/i);
  assert.match(html, /<b[ >]/);
  validatePNG(await readFile(join(artifacts, 'page.png')));
  console.log(`PASS real-app diagnostic buffer: baseline, overflow, hydration stack, DOM, HTML, PNG (${output})`);
} finally {
  for (const request of requests) request.destroy();
  if (server) {
    server.closeAllConnections();
    await new Promise(resolve => server.close(resolve));
  }
  process.off('SIGINT', interrupt);
  process.off('SIGTERM', interrupt);
}
