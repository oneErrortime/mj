// Basic smoke-test for MoleculerRsClient (Node built-in test runner, no deps).
// Run:  node --test test/client.test.js
//
// These tests run in "offline" mode — they mock the global fetch to avoid
// requiring a live moleculer-rs instance.
"use strict";

const { describe, it, mock, before, after } = require("node:test");
const assert = require("node:assert/strict");

const { MoleculerRsClient, MoleculerRsError } = require("../src/index.js");

// ── helpers ──────────────────────────────────────────────────────────────────

function mockFetch(statusCode, body) {
  const response = {
    ok: statusCode >= 200 && statusCode < 300,
    status: statusCode,
    headers: { get: () => "application/json" },
    json: async () => body,
    text: async () => JSON.stringify(body),
  };
  globalThis.fetch = async () => response;
}

// ── tests ────────────────────────────────────────────────────────────────────

describe("MoleculerRsClient constructor", () => {
  it("uses defaults", () => {
    const c = new MoleculerRsClient();
    assert.equal(c.endpoint, "http://localhost:3210");
    assert.equal(c.token, null);
    assert.equal(c.timeout, 10_000);
  });

  it("accepts custom options", () => {
    const c = new MoleculerRsClient({ endpoint: "http://myhost:9000/", token: "abc", timeout: 5000 });
    assert.equal(c.endpoint, "http://myhost:9000");
    assert.equal(c.token, "abc");
    assert.equal(c.timeout, 5000);
  });
});

describe("MoleculerRsClient.call()", () => {
  it("resolves on 200", async () => {
    mockFetch(200, { result: 8 });
    const c = new MoleculerRsClient();
    const res = await c.call("math.add", { a: 5, b: 3 });
    assert.deepEqual(res, { result: 8 });
  });

  it("throws MoleculerRsError on 4xx", async () => {
    mockFetch(404, { message: "Action not found" });
    const c = new MoleculerRsClient();
    await assert.rejects(() => c.call("missing.action"), MoleculerRsError);
  });
});

describe("MoleculerRsClient.sendToChannel()", () => {
  it("resolves on 200", async () => {
    mockFetch(200, { ok: true });
    const c = new MoleculerRsClient();
    const res = await c.sendToChannel("orders.created", { id: "ORD-1" });
    assert.deepEqual(res, { ok: true });
  });
});

describe("MoleculerRsClient.health()", () => {
  it("resolves broker health", async () => {
    mockFetch(200, { ok: true, node_id: "node-main" });
    const c = new MoleculerRsClient();
    const res = await c.health();
    assert.equal(res.ok, true);
  });
});
