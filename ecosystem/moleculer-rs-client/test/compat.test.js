// Tests for the ServiceBroker shim in compat.js
"use strict";

const { describe, it, before, after } = require("node:test");
const assert = require("node:assert/strict");

const {
  ServiceBroker, Context, Errors, Serializers, METRIC, Utils,
  ValidationError, ServiceSchemaError, BrokerOptionsError,
} = require("../src/compat.js");

// ── Error classes ─────────────────────────────────────────────────────────────

describe("Errors", () => {
  it("ValidationError instanceof chain", () => {
    const err = new ValidationError("bad", "TEST", []);
    assert.ok(err instanceof ValidationError);
    assert.ok(err instanceof Errors.MoleculerClientError);
    assert.ok(err instanceof Errors.MoleculerError);
    assert.equal(err.code, 422);
    assert.equal(err.name, "ValidationError");
  });

  it("Errors flat exports match .Errors namespace", () => {
    assert.equal(ValidationError, Errors.ValidationError);
    assert.equal(BrokerOptionsError, Errors.BrokerOptionsError);
    assert.equal(ServiceSchemaError, Errors.ServiceSchemaError);
  });
});

// ── Context ───────────────────────────────────────────────────────────────────

describe("Context", () => {
  it("Context.create populates fields", () => {
    const broker = new ServiceBroker({ logger: false });
    const ctx = Context.create(broker, null, { x: 1 }, {});
    assert.equal(ctx.params.x, 1);
    assert.ok(ctx.id);
    assert.equal(ctx.nodeID, broker.nodeID);
  });
});

// ── Serializers ───────────────────────────────────────────────────────────────

describe("Serializers", () => {
  it("JSON serializer round-trips objects", () => {
    const s = Serializers.resolve("JSON");
    const buf = s.serialize({ a: 1 });
    assert.deepEqual(s.deserialize(buf), { a: 1 });
  });
});

// ── Promise helpers ───────────────────────────────────────────────────────────

describe("Promise.mapSeries", () => {
  it("processes items in order", async () => {
    const results = await Promise.mapSeries([1, 2, 3], x => Promise.resolve(x * 2));
    assert.deepEqual(results, [2, 4, 6]);
  });
});

describe("Promise.method", () => {
  it("wraps sync function returning a Promise", async () => {
    const fn = Promise.method((x) => x * 3);
    const r = await fn(4);
    assert.equal(r, 12);
  });

  it("catches thrown errors and rejects", async () => {
    const fn = Promise.method(() => { throw new Error("boom"); });
    await assert.rejects(fn, /boom/);
  });
});

// ── ServiceBroker ─────────────────────────────────────────────────────────────

describe("ServiceBroker", () => {
  it("has expected properties", () => {
    const b = new ServiceBroker({ logger: false, nodeID: "test-node" });
    assert.equal(b.nodeID, "test-node");
    assert.equal(b.namespace, "");
    assert.equal(b.isMetricsEnabled(), false);
    assert.equal(b.isTracingEnabled(), false);
  });

  it("getLogger returns a silent logger when logger:false", () => {
    const b = new ServiceBroker({ logger: false });
    const l = b.getLogger("Test");
    assert.equal(typeof l.info, "function");
    assert.doesNotThrow(() => l.info("test"));
  });

  it("createService returns a service with methods bound", () => {
    const b = new ServiceBroker({ logger: false });
    const svc = b.createService({
      name: "math",
      methods: { add(a, b) { return a + b; } },
    });
    assert.equal(svc.name, "math");
    assert.equal(svc.add(2, 3), 5);
  });

  it("start / stop lifecycle works", async () => {
    const order = [];
    const mw = {
      created() { order.push("mw:created"); },
      starting() { order.push("mw:starting"); },
      stopping() { order.push("mw:stopping"); },
      stopped()  { order.push("mw:stopped"); },
    };

    const b = new ServiceBroker({ logger: false, middlewares: [mw] });
    b.createService({
      name: "dummy",
      created()  { order.push("svc:created"); },
      started()  { order.push("svc:started"); },
      stopped()  { order.push("svc:stopped"); },
    });

    await b.start();
    await b.stop();

    assert.ok(order.includes("mw:created"),  "mw:created");
    assert.ok(order.includes("mw:starting"), "mw:starting");
    assert.ok(order.includes("svc:started"), "svc:started");
    assert.ok(order.includes("mw:stopping"), "mw:stopping");
    assert.ok(order.includes("svc:stopped"), "svc:stopped");
  });

  it("emit delivers event to registered service", async () => {
    const b = new ServiceBroker({ logger: false });
    let received = null;
    b.createService({
      name: "listener",
      events: {
        "test.event": function(ctx) { received = ctx.params; },
      },
    });
    b.emit("test.event", { payload: { hello: "world" } }, {});
    await new Promise(r => setTimeout(r, 10));
    assert.ok(received);
  });

  it("applyMixins merges methods correctly", () => {
    const b = new ServiceBroker({ logger: false });
    const mixin = { name: "mixin", methods: { greet() { return "hi"; } } };
    const svc = b.createService({
      name: "child",
      mixins: [mixin],
      methods: { add(a, b) { return a + b; } },
    });
    assert.equal(svc.greet(), "hi");
    assert.equal(svc.add(1, 2), 3);
  });
});
