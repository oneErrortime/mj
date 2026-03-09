// moleculer-rs-client/src/compat.js
//
// Lightweight compatibility shim that provides the subset of the Node.js
// `moleculer` package API actually used by the ecosystem packages.
// This lets us drop the original moleculer peer dependency entirely.
"use strict";

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
  MoleculerError, MoleculerRetryableError, MoleculerClientError,
  BrokerOptionsError, ServiceSchemaError, ValidationError,
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
    throw new ServiceSchemaError(`Unknown serializer: "${type}". moleculer-rs-client only supports JSON.`);
  }
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
    this.broker  = broker;
    this.params  = params || {};
    this.meta    = opts.meta || {};
    this.id      = opts.id  || crypto.randomUUID();
    this.level   = opts.level || 1;
  }

  static create(broker, endpoint, params, opts) {
    return new Context(broker, endpoint, params, opts);
  }
}

// ─── Utils ─────────────────────────────────────────────────────────────────────

function isFunction(fn)     { return typeof fn === "function"; }
function isPlainObject(obj) { return obj !== null && typeof obj === "object" && !Array.isArray(obj); }
function safetyObject(obj, ...paths) {
  // recursively masks sensitive keys (passwords, tokens …)
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

module.exports = { Errors, Serializers, METRIC, Context, Utils };
