import test from 'node:test';
import assert from 'node:assert/strict';
import { createDiagnosticBuffer } from './chart-diagnostics.mjs';

test('late DOM warning, pageerror, and failure survive earlier and later noise', () => {
  const { entries, record } = createDiagnosticBuffer();
  for (let index = 0; index < 310; index += 1) {
    assert.equal(record({ type: 'log', index }), true);
    assert.ok(entries.length <= 300);
  }
  const critical = [
    { type: 'warning', text: 'hydration mismatch', arguments: [{ outerHTML: '<div>DOM</div>' }] },
    { type: 'pageerror', message: 'hydration failed' },
    { type: 'failure', message: 'smoke assertion failed' },
  ];
  for (const entry of critical) assert.equal(record(entry), true);
  for (let index = 0; index < 1000; index += 1) {
    assert.equal(record({ type: 'log', index }), true);
    assert.equal(entries.length, 300);
    for (const entry of critical) assert.ok(entries.includes(entry));
  }
});

test('all priority tiers reject lower priorities and accept equal or higher ones', () => {
  const tiers = [['log', 'info', 'debug'], ['diagnostic-error'], ['error', 'warning'], ['pageerror', 'http-error'], ['failure']];
  for (const [existingRank, existingTypes] of tiers.entries()) {
    for (const existingType of existingTypes) {
      for (const [incomingRank, incomingTypes] of tiers.entries()) {
        for (const incomingType of incomingTypes) {
          const { entries, record } = createDiagnosticBuffer(1);
          const existing = { type: existingType };
          const incoming = { type: incomingType };
          assert.equal(record(existing), true);
          const accepted = incomingRank >= existingRank;
          assert.equal(record(incoming), accepted, `${existingType} -> ${incomingType}`);
          assert.equal(entries.length, 1);
          assert.strictEqual(entries[0], accepted ? incoming : existing);
        }
      }
    }
  }
});

test('evicts the oldest lowest-priority peer while preserving survivor order', () => {
  const { entries, record } = createDiagnosticBuffer(4);
  const failure = { type: 'failure' };
  const oldest = { type: 'warning', id: 1 };
  const pageerror = { type: 'pageerror' };
  const newer = { type: 'error', id: 2 };
  for (const entry of [failure, oldest, pageerror, newer]) record(entry);
  const incoming = { type: 'warning', id: 3 };
  assert.equal(record(incoming), true);
  assert.deepEqual(entries, [failure, pageerror, newer, incoming]);
  assert.equal(record({ type: 'log' }), false);
  assert.deepEqual(entries, [failure, pageerror, newer, incoming]);
});

test('equal-priority saturation is bounded and FIFO', () => {
  const { entries, record } = createDiagnosticBuffer(3);
  for (let id = 0; id < 1000; id += 1) {
    assert.equal(record({ type: 'failure', id }), true);
    assert.equal(entries.length, Math.min(id + 1, 3));
    if (id >= 2) assert.equal(record({ type: 'log' }), false);
  }
  assert.deepEqual(entries.map(entry => entry.id), [997, 998, 999]);
});

test('retains array and entry references for asynchronous enrichment', async () => {
  const buffer = createDiagnosticBuffer(2);
  const entries = buffer.entries;
  const entry = { type: 'warning', arguments: [] };
  assert.equal(buffer.record(entry), true);
  buffer.record({ type: 'log' });
  buffer.record({ type: 'pageerror' });
  await Promise.resolve().then(() => { entry.arguments.push({ outerHTML: '<main>hydration</main>' }); });
  assert.strictEqual(buffer.entries, entries);
  assert.strictEqual(entries[0], entry);
  assert.deepEqual(entries[0].arguments, [{ outerHTML: '<main>hydration</main>' }]);
});

test('requires positive safe integer capacities', () => {
  for (const limit of [0, -1, 1.5, NaN, Infinity, -Infinity, Number.MAX_SAFE_INTEGER + 1, '3', null, true, 1n, {}, []]) {
    assert.throws(() => createDiagnosticBuffer(limit), {
      name: 'RangeError',
      message: /positive safe integer/,
    });
  }
  for (const limit of [1, 300, Number.MAX_SAFE_INTEGER]) {
    const { entries, record } = createDiagnosticBuffer(limit);
    assert.deepEqual(entries, []);
    assert.equal(record({ type: 'log' }), true);
  }
});
