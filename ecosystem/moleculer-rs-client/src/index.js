// moleculer-rs-client
// Node.js HTTP client for the moleculer-rs Rust broker (Laboratory Agent HTTP API).
// Zero runtime dependencies — uses the built-in `fetch` available since Node 18.
"use strict";

const DEFAULT_ENDPOINT = "http://localhost:3210";
const DEFAULT_TIMEOUT_MS = 10_000;

class MoleculerRsError extends Error {
  constructor(message, status, body) {
    super(message);
    this.name = "MoleculerRsError";
    this.status = status;
    this.body = body;
  }
}

/**
 * MoleculerRsClient
 *
 * Thin HTTP wrapper around the moleculer-rs Laboratory Agent API.
 * All methods are async and return plain JS objects parsed from JSON.
 *
 * @example
 * const { MoleculerRsClient } = require("moleculer-rs-client");
 * const broker = new MoleculerRsClient({ endpoint: "http://localhost:3210" });
 * const result = await broker.call("math.add", { a: 5, b: 3 });
 * console.log(result); // { result: 8 }
 */
class MoleculerRsClient {
  /**
   * @param {object} [options]
   * @param {string} [options.endpoint="http://localhost:3210"]
   * @param {string|null} [options.token=null]   Bearer token (if broker was started with one)
   * @param {number} [options.timeout=10000]     Request timeout in ms
   */
  constructor(options = {}) {
    this.endpoint = (options.endpoint || DEFAULT_ENDPOINT).replace(/\/$/, "");
    this.token = options.token || null;
    this.timeout = options.timeout ?? DEFAULT_TIMEOUT_MS;
  }

  // ─── Internal helpers ──────────────────────────────────────────────────────

  _headers(extra = {}) {
    const h = { Accept: "application/json", ...extra };
    if (this.token) h["Authorization"] = `Bearer ${this.token}`;
    return h;
  }

  async _fetch(path, init = {}) {
    const url = this.endpoint + path;
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), this.timeout);

    let res;
    try {
      res = await fetch(url, { ...init, signal: controller.signal, headers: this._headers(init.headers) });
    } catch (err) {
      if (err.name === "AbortError")
        throw new MoleculerRsError(`Request to ${path} timed out after ${this.timeout}ms`, null, null);
      throw err;
    } finally {
      clearTimeout(timer);
    }

    let body;
    const ct = res.headers.get("content-type") || "";
    if (ct.includes("application/json")) {
      body = await res.json();
    } else {
      body = await res.text();
    }

    if (!res.ok) {
      const msg = (body && body.message) || (typeof body === "string" ? body : `HTTP ${res.status}`);
      throw new MoleculerRsError(msg, res.status, body);
    }

    return body;
  }

  async _get(path) {
    return this._fetch(path, { method: "GET" });
  }

  async _post(path, data) {
    return this._fetch(path, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(data),
    });
  }

  async _delete(path) {
    return this._fetch(path, { method: "DELETE" });
  }

  // ─── Broker info ───────────────────────────────────────────────────────────

  /** Check if the broker is alive. Returns `{ ok: true, ... }` */
  health() { return this._get("/health"); }

  /** Broker metadata (node_id, namespace, features, version, …) */
  info() { return this._get("/info"); }

  // ─── Registry ──────────────────────────────────────────────────────────────

  /** List all registered services */
  services() { return this._get("/services"); }

  /** List all registered actions */
  actions() { return this._get("/actions"); }

  /** Service call-graph topology */
  topology() { return this._get("/topology"); }

  // ─── Observability ─────────────────────────────────────────────────────────

  /** Current metrics snapshot (all counters / gauges / histograms) */
  metrics() { return this._get("/metrics"); }

  /** Recent distributed trace spans */
  traces() { return this._get("/traces"); }

  /** Recent log lines from the in-memory ring buffer */
  logs() { return this._get("/logs"); }

  /** Prometheus-format metrics (plain text) */
  metricsPrometheus() { return this._get("/metrics/prometheus"); }

  // ─── Circuit breakers & cache ─────────────────────────────────────────────

  /** Circuit-breaker states for all actions */
  circuitBreakers() { return this._get("/circuit-breakers"); }

  /** LRU cache entries */
  cache() { return this._get("/cache"); }

  // ─── Channels ──────────────────────────────────────────────────────────────

  /** List channels, consumer groups, pending counts and DLQ entries */
  channels() { return this._get("/channels"); }

  /**
   * Publish a message to a channel.
   * @param {string} channel  Channel name (e.g. "orders.created")
   * @param {object} payload
   */
  sendToChannel(channel, payload) {
    return this._post("/channels/send", { channel, payload });
  }

  /**
   * Retry or discard a Dead Letter Queue entry.
   * @param {string} id  DLQ entry ID
   */
  retryDlq(id) { return this._delete(`/channels/dlq/${encodeURIComponent(id)}`); }

  // ─── Action calling ────────────────────────────────────────────────────────

  /**
   * Call a service action.
   * @param {string} action   Fully-qualified action name, e.g. "math.add"
   * @param {object} [params] Action parameters
   * @returns {Promise<any>}  Action result
   *
   * @example
   * const res = await broker.call("math.add", { a: 1, b: 2 });
   * // res === { result: 3 }
   */
  call(action, params = {}) {
    return this._post("/action", { action, params });
  }

  /**
   * Emit an event (fire-and-forget).
   * NOTE: moleculer-rs does not yet expose a dedicated /emit endpoint, so this
   * is a best-effort wrapper that calls the "broker.emit" meta-action if available,
   * otherwise throws a descriptive error.
   *
   * @param {string} event
   * @param {object} [payload]
   */
  async emit(event, payload = {}) {
    try {
      return await this._post("/action", { action: "broker.emit", params: { event, payload } });
    } catch (err) {
      if (err.status === 404) {
        throw new MoleculerRsError(
          "broker.emit action not found — moleculer-rs does not yet expose a /emit endpoint. " +
          "Use sendToChannel() for durable messaging.",
          404,
          null
        );
      }
      throw err;
    }
  }
}

module.exports = { MoleculerRsClient, MoleculerRsError };
