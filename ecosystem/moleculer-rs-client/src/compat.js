// moleculer-rs-client/src/compat.js
//
// Compatibility shim — provides the subset of the Node.js `moleculer`
// package API actually used by the ecosystem packages, including a full
// ServiceBroker shim so tests and packages no longer need the real
// moleculer peer dependency.
"use strict";

const crypto = require("crypto");

// ─── Errors ────────────────────────────────────────────────────────────────────

class MoleculerError extends Error {
  constructor(message, code, type, data) {
    super(message);
    this.name = "MoleculerError";
    this.code = code || 500;
    this.type = type || "MOLECULER_ERROR";
    this.data = data || null;
    if (Error.captureStackTrace) Error.captureStackTrace(this, this.constructor);
  }
}

class MoleculerRetryableError extends MoleculerError {
  constructor(message, code, type, data) {
    super(message, code || 500, type || "MOLECULER_RETRYABLE_ERROR", data);
    this.name = "MoleculerRetryableError";
    this.retryable = true;
  }
}

class BrokerOptionsError extends MoleculerError {
  constructor(message, data) {
    super(message, 500, "BROKER_OPTIONS_ERROR", data);
    this.name = "BrokerOptionsError";
  }
}

class ServiceSchemaError extends MoleculerError {
  constructor(message, data) {
    super(message, 500, "SERVICE_SCHEMA_ERROR", data);
    this.name = "ServiceSchemaError";
  }
}

class MoleculerClientError extends MoleculerError {
  constructor(message, code, type, data) {
    super(message, code || 422, type || "MOLECULER_CLIENT_ERROR", data);
    this.name = "MoleculerClientError";
  }
}

class ValidationError extends MoleculerClientError {
  constructor(message, type, data) {
    super(message, 422, type || "VALIDATION_ERROR", data);
    this.name = "ValidationError";
  }
}

const Errors = {
  MoleculerError,
  MoleculerRetryableError,
  MoleculerClientError,
  BrokerOptionsError,
  ServiceSchemaError,
  ValidationError,
};

// ─── Serializer ────────────────────────────────────────────────────────────────

class JsonSerializer {
  init() {}
  serialize(obj)   { return Buffer.from(JSON.stringify(obj)); }
  deserialize(buf) { return JSON.parse(buf.toString()); }
}

const Serializers = {
  resolve(type) {
    if (!type || type === "JSON") return new JsonSerializer();
    throw new ServiceSchemaError(
      `Unknown serializer: "${type}". moleculer-rs-client only supports JSON.`
    );
  },
};

// ─── Metrics constants ─────────────────────────────────────────────────────────

const METRIC = {
  TYPE_COUNTER:   "counter",
  TYPE_GAUGE:     "gauge",
  TYPE_HISTOGRAM: "histogram",
  TYPE_INFO:      "info",
};

// ─── Context ───────────────────────────────────────────────────────────────────

class Context {
  constructor(broker, endpoint, params, opts = {}) {
    this.broker    = broker;
    this.params    = params || {};
    this.meta      = opts.meta || {};
    this.id        = opts.id  || crypto.randomUUID();
    this.requestID = opts.requestID || this.id;
    this.level     = opts.level || 1;
    this.tracing   = opts.tracing || false;
    this.headers   = opts.headers || null;
    this.service   = opts.service || null;
    this.nodeID    = broker ? broker.nodeID : null;
    if (opts.parentCtx) {
      this.parentID  = opts.parentCtx.id;
      this.requestID = opts.parentCtx.requestID || this.id;
      this.level     = (opts.parentCtx.level || 0) + 1;
      this.tracing   = opts.parentCtx.tracing;
    }
    if (opts.caller) this.caller = opts.caller;
  }

  static create(broker, endpoint, params, opts) {
    return new Context(broker, endpoint, params, opts);
  }
}

// ─── Logger ────────────────────────────────────────────────────────────────────

function makeLogger(name, silent) {
  if (silent) {
    return {
      trace: () => {}, debug: () => {}, info: () => {},
      warn:  () => {}, error: () => {}, fatal: () => {},
    };
  }
  const prefix = `[${name}]`;
  return {
    trace: (...a) => console.log  (prefix, "TRACE", ...a),
    debug: (...a) => console.log  (prefix, "DEBUG", ...a),
    info:  (...a) => console.info (prefix, ...a),
    warn:  (...a) => console.warn (prefix, ...a),
    error: (...a) => console.error(prefix, ...a),
    fatal: (...a) => console.error(prefix, "FATAL", ...a),
  };
}

// ─── Promise helpers ──────────────────────────────────────────────────────────

const RsPromise = Object.assign(Promise, {
  mapSeries(arr, fn) {
    return arr.reduce(
      (chain, item) => chain.then(results =>
        Promise.resolve(fn(item)).then(r => { results.push(r); return results; })
      ),
      Promise.resolve([])
    );
  },
  method(fn) {
    return function (...args) {
      try {
        return Promise.resolve(fn.apply(this, args));
      } catch (err) {
        return Promise.reject(err);
      }
    };
  },
});

// ─── No-op Metrics registry ────────────────────────────────────────────────────

const noopMetrics = {
  register:  () => {},
  increment: () => {},
  decrement: () => {},
  timer:     () => () => {},
  set:       () => {},
};

// ─── No-op Tracer ──────────────────────────────────────────────────────────────

const noopTracer = {
  startSpan: () => ({ finish: () => {}, addTags: () => {}, logKV: () => {} }),
};

// ─── Middleware handler ────────────────────────────────────────────────────────

class MiddlewareHandler {
  constructor() { this._list = []; }
  add(mw) { this._list.push(mw); }
  wrapHandler(type, handler) {
    return this._list.reduce((h, mw) => {
      if (typeof mw[type] === "function") return mw[type](h);
      return h;
    }, handler);
  }
}

// ─── Service ──────────────────────────────────────────────────────────────────

class Service {
  constructor(broker, schema) {
    this.broker   = broker;
    this.schema   = schema;
    this.name     = schema.name;
    this.version  = schema.version || undefined;
    this.fullName = schema.fullName ||
      (this.version ? `v${this.version}.${this.name}` : this.name);
    this.settings = schema.settings || {};
    this.metadata = schema.metadata || {};
    this.logger   = broker.getLogger(this.fullName);

    const methods = schema.methods || {};
    for (const [key, fn] of Object.entries(methods)) {
      if (typeof fn === "function") this[key] = fn.bind(this);
    }
  }
}

// ─── Mixin merger ─────────────────────────────────────────────────────────────

function applyMixins(schema) {
  const mixins = schema.mixins;
  if (!mixins || !mixins.length) return schema;

  const LIFECYCLE  = new Set(["created", "started", "stopped", "merged"]);
  const MERGE_OBJ  = new Set(["settings", "metadata", "methods", "actions",
                               "events", "channels", "hooks"]);

  const base = Object.assign({}, schema);
  delete base.mixins;

  for (const mixin of mixins) {
    const resolved = applyMixins(mixin);
    for (const [key, value] of Object.entries(resolved)) {
      if (LIFECYCLE.has(key)) {
        const existing = base[key];
        if (!existing)           base[key] = [value];
        else if (Array.isArray(existing)) existing.push(value);
        else                     base[key] = [existing, value];
      } else if (MERGE_OBJ.has(key)) {
        base[key] = Object.assign({}, value, base[key] || {});
      } else if (!(key in base)) {
        base[key] = value;
      }
    }
  }

  return base;
}

// ─── ServiceBroker shim ───────────────────────────────────────────────────────

class ServiceBroker {
  constructor(opts = {}) {
    this.opts      = opts;
    this.nodeID    = opts.nodeID    || `rs-node-${crypto.randomUUID().slice(0, 8)}`;
    this.namespace = opts.namespace || "";
    this.Promise   = RsPromise;

    const silent   = opts.logger === false;
    this._silent   = silent;
    this.logger    = makeLogger("Broker", silent);

    this.metrics     = noopMetrics;
    this.tracer      = noopTracer;
    this.cacher      = null;
    this._services   = [];
    this._started    = false;
    this.middlewares = new MiddlewareHandler();

    // Register middlewares from options (they'll be init'd in start())
    this._pendingMiddlewares = opts.middlewares || [];
  }

  // ─── Logger ────────────────────────────────────────────────────────────────

  getLogger(name) { return makeLogger(name, this._silent); }
  getConstructorName(obj) { return obj && obj.constructor ? obj.constructor.name : "Unknown"; }

  // ─── Feature flags ────────────────────────────────────────────────────────

  isMetricsEnabled() { return false; }
  isTracingEnabled() { return false; }

  // ─── Method wrapping ──────────────────────────────────────────────────────

  wrapMethod(name, fn) { return this.middlewares.wrapHandler(name, fn); }

  // ─── Service management ───────────────────────────────────────────────────

  createService(schema) {
    const merged = applyMixins(schema);
    const svc    = new Service(this, merged);
    this._services.push(svc);

    this._callSync(svc, "created");

    for (const mw of this.middlewares._list) {
      if (typeof mw.serviceCreated === "function") {
        Promise.resolve(mw.serviceCreated(svc)).catch(err =>
          this.logger.error(`Middleware serviceCreated error for '${svc.fullName}'`, err)
        );
      }
    }

    return svc;
  }

  async destroyService(svc) {
    await this._callAsync(svc, "stopped");
    for (const mw of this.middlewares._list) {
      if (typeof mw.serviceStopping === "function")
        await Promise.resolve(mw.serviceStopping(svc)).catch(() => {});
    }
    this._services = this._services.filter(s => s !== svc);
  }

  // ─── Lifecycle ────────────────────────────────────────────────────────────

  _callSync(svc, hook) {
    const fns = svc.schema[hook];
    if (!fns) return;
    for (const fn of (Array.isArray(fns) ? fns : [fns])) {
      if (typeof fn === "function") { try { fn.call(svc); } catch (_) {} }
    }
  }

  async _callAsync(svc, hook) {
    const fns = svc.schema[hook];
    if (!fns) return;
    for (const fn of (Array.isArray(fns) ? fns : [fns])) {
      if (typeof fn === "function") {
        try { await Promise.resolve(fn.call(svc)); } catch (_) {}
      }
    }
  }

  async start() {
    if (this._started) return;

    // Init middlewares — call created() on each with the broker
    for (const mw of this._pendingMiddlewares) {
      this.middlewares.add(mw);
      if (typeof mw.created === "function") {
        try { await Promise.resolve(mw.created(this)); }
        catch (e) { this.logger.error("Middleware created hook failed", e); }
      }
    }

    // Notify middlewares that previously-created services exist
    for (const svc of this._services) {
      for (const mw of this.middlewares._list) {
        if (typeof mw.serviceCreated === "function")
          await Promise.resolve(mw.serviceCreated(svc)).catch(() => {});
      }
    }

    // Service started hooks
    for (const svc of this._services) await this._callAsync(svc, "started");

    // Middleware starting/started
    for (const mw of this.middlewares._list) {
      if (typeof mw.starting === "function")
        await Promise.resolve(mw.starting()).catch(e => this.logger.error(e));
    }
    for (const mw of this.middlewares._list) {
      if (typeof mw.started === "function")
        await Promise.resolve(mw.started()).catch(e => this.logger.error(e));
    }

    this._started = true;
  }

  async stop() {
    if (!this._started) return;

    for (const mw of this.middlewares._list) {
      if (typeof mw.stopping === "function")
        await Promise.resolve(mw.stopping()).catch(() => {});
    }

    for (const svc of [...this._services].reverse()) {
      await this._callAsync(svc, "stopped");
      for (const mw of this.middlewares._list) {
        if (typeof mw.serviceStopping === "function")
          await Promise.resolve(mw.serviceStopping(svc)).catch(() => {});
      }
    }

    for (const mw of this.middlewares._list) {
      if (typeof mw.stopped === "function")
        await Promise.resolve(mw.stopped()).catch(() => {});
    }

    this._started = false;
  }

  // ─── Event bus ────────────────────────────────────────────────────────────

  emit(event, payload, opts) {
    for (const svc of this._services) {
      const events = svc.schema.events;
      if (!events) continue;
      const handler = events[event];
      if (!handler) continue;
      const fn = typeof handler === "function" ? handler : handler.handler;
      if (typeof fn !== "function") continue;
      const ctx = Context.create(this, null, { payload, headers: payload && payload.headers }, {});
      Promise.resolve(fn.call(svc, ctx)).catch(() => {});
    }
  }

  broadcast(event, payload, opts) { return this.emit(event, payload, opts); }

  fatal(message, err) { this.logger.fatal(message, err); }
}

// ─── Utils ────────────────────────────────────────────────────────────────────

function isFunction(fn)     { return typeof fn === "function"; }
function isPlainObject(obj) { return obj !== null && typeof obj === "object" && !Array.isArray(obj); }
function safetyObject(obj) {
  const SENSITIVE = new Set(["password", "passwd", "secret", "token", "apiKey", "api_key"]);
  function mask(o) {
    if (!isPlainObject(o)) return o;
    return Object.fromEntries(Object.entries(o).map(([k, v]) =>
      [k, SENSITIVE.has(k.toLowerCase()) ? "***" : mask(v)]
    ));
  }
  return mask(obj);
}

const Utils = { isFunction, isPlainObject, safetyObject };

// ─── Exports ──────────────────────────────────────────────────────────────────

module.exports = {
  ServiceBroker,
  Service,
  Context,
  Errors,
  Serializers,
  METRIC,
  Utils,
  // Flat error exports (convenience — mirrors require("moleculer").Errors.*)
  ...Errors,
};
