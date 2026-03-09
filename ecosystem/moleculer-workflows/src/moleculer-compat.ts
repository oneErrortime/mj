/**
 * moleculer-compat.ts
 *
 * Lightweight stand-in for the `moleculer` Node.js package types.
 * Used by moleculer-workflows so it can run without the full moleculer package.
 */

// ─── Errors ──────────────────────────────────────────────────────────────────

export class MoleculerError extends Error {
  code: number;
  type: string;
  data: unknown;
  constructor(message: string, code = 500, type = "MOLECULER_ERROR", data: unknown = null) {
    super(message);
    this.name = "MoleculerError";
    this.code = code;
    this.type = type;
    this.data = data;
  }
}

export class MoleculerRetryableError extends MoleculerError {
  retryable = true;
  constructor(message: string, code = 500, type = "MOLECULER_RETRYABLE_ERROR", data: unknown = null) {
    super(message, code, type, data);
    this.name = "MoleculerRetryableError";
  }
}

export class MoleculerClientError extends MoleculerError {
  constructor(message: string, code = 422, type = "MOLECULER_CLIENT_ERROR", data: unknown = null) {
    super(message, code, type, data);
    this.name = "MoleculerClientError";
  }
}

export class ServiceSchemaError extends MoleculerError {
  constructor(message: string, data: unknown = null) {
    super(message, 500, "SERVICE_SCHEMA_ERROR", data);
    this.name = "ServiceSchemaError";
  }
}

export class ValidationError extends MoleculerClientError {
  constructor(message: string, type = "VALIDATION_ERROR", data: unknown = null) {
    super(message, 422, type, data);
    this.name = "ValidationError";
  }
}

export class BrokerOptionsError extends MoleculerError {
  constructor(message: string, data: unknown = null) {
    super(message, 500, "BROKER_OPTIONS_ERROR", data);
    this.name = "BrokerOptionsError";
  }
}

export const Errors = {
  MoleculerError, MoleculerRetryableError, MoleculerClientError,
  BrokerOptionsError, ServiceSchemaError, ValidationError,
};

// Errors namespace (for `Errors.PlainMoleculerError` and similar type usage)
export namespace Errors {
  export type PlainMoleculerError = {
    name: string;
    message: string;
    code?: number;
    type?: string;
    data?: unknown;
    stack?: string;
  };
}

// ─── Metrics constants ────────────────────────────────────────────────────────

export const METRIC = {
  TYPE_COUNTER:   "counter",
  TYPE_GAUGE:     "gauge",
  TYPE_HISTOGRAM: "histogram",
  TYPE_INFO:      "info",
} as const;

// ─── Serializers ─────────────────────────────────────────────────────────────

export class JsonSerializer {
  init(_broker?: unknown) {}
  serialize(obj: unknown): Buffer { return Buffer.from(JSON.stringify(obj)); }
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  deserialize(buf: Buffer): any { return JSON.parse(buf.toString()); }
}

export namespace Serializers {
  export function resolve(type?: string): JsonSerializer {
    if (!type || type === "JSON") return new JsonSerializer();
    throw new ServiceSchemaError(`Unknown serializer: "${type}". Only JSON is supported.`);
  }
  export type Base = JsonSerializer;
}

// ─── Utils ────────────────────────────────────────────────────────────────────

export const Utils = {
  isFunction: (fn: unknown): fn is Function => typeof fn === "function",
  isPlainObject: (obj: unknown): obj is Record<string, unknown> =>
    obj !== null && typeof obj === "object" && !Array.isArray(obj),
  safetyObject: (obj: unknown): Record<string, unknown> => {
    const SENSITIVE = new Set(["password", "passwd", "secret", "token", "apikey", "api_key"]);
    function mask(o: unknown): unknown {
      if (o === null || typeof o !== "object" || Array.isArray(o)) return o;
      return Object.fromEntries(
        Object.entries(o as Record<string, unknown>).map(([k, v]) =>
          [k, SENSITIVE.has(k.toLowerCase()) ? "***" : mask(v)]
        )
      );
    }
    return mask(obj) as Record<string, unknown>;
  },
  makeDirs: (_path: string): void => { /* no-op in compat */ },
};

// ─── Minimal stub types used by moleculer-workflows ──────────────────────────
//
// These interfaces match the shapes moleculer-workflows actually reads/writes,
// not the full Moleculer API. When running against a real Moleculer broker
// (Node.js) they are overridden at runtime anyway.

export interface Logger {
  debug(...args: unknown[]): void;
  info(...args: unknown[]): void;
  warn(...args: unknown[]): void;
  error(...args: unknown[]): void;
  fatal(...args: unknown[]): void;
}

export interface Span {
  id: string;
  name: string;
  startTime: number;
  endTime?: number;
  tags?: Record<string, unknown>;
  finish(tags?: Record<string, unknown>): void;
  setError(err: Error): void;
}

export interface Tracer {
  startSpan(name: string, opts?: Record<string, unknown>): Span;
  getCurrentTraceID?(): string | null;
  getActiveSpanID?(): string | null;
}

export interface Context<P = Record<string, unknown>, M = Record<string, unknown>> {
  id: string;
  broker: ServiceBroker;
  service?: Service;
  params: P;
  meta: M;
  level: number;
  span?: Span;
  tracing?: boolean;
  startSpan(name: string, opts?: Record<string, unknown>): Span;
  finishSpan(span: Span, time?: number): void;
  call<T = unknown>(action: string, params?: unknown, opts?: unknown): Promise<T>;
  emit(event: string, payload?: unknown): Promise<void>;
}

export interface ActionSchema {
  name?: string;
  handler?: (ctx: Context) => Promise<unknown>;
  cache?: boolean | Record<string, unknown>;
  params?: Record<string, unknown>;
}

export interface ServiceSchema {
  name: string;
  version?: string | number;
  actions?: Record<string, ActionSchema | ((ctx: Context) => Promise<unknown>)>;
  events?: Record<string, unknown>;
  methods?: Record<string, unknown>;
  mixins?: ServiceSchema[];
  [key: string]: unknown;
}

export interface Service {
  name: string;
  version?: string | number;
  broker: ServiceBroker;
  logger: Logger;
  schema: ServiceSchema;
  [key: string]: unknown;
}

export interface Middleware {
  name?: string;
  [key: string]: unknown;
}

export interface ServiceBroker {
  nodeID: string;
  instanceID?: string;
  namespace: string;
  logger: Logger;
  tracer: Tracer;
  Promise: PromiseConstructor;
  validator?: { compile(schema: unknown): (obj: unknown) => boolean | unknown[] };
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  middlewares?: any;
  metrics: {
    isEnabled(): boolean;
    register(opts: Record<string, unknown>): void;
    increment(name: string, labels?: Record<string, unknown>, value?: number): void;
    decrement(name: string, labels?: Record<string, unknown>, value?: number): void;
    set(name: string, value: number, labels?: Record<string, unknown>): void;
    timer?(name: string, labels?: Record<string, unknown>): () => void;
  };
  isMetricsEnabled(): boolean;
  isTracingEnabled(): boolean;
  getConstructorName(obj: unknown): string;
  generateUid?(): string;
  call<T = unknown>(action: string, params?: unknown, opts?: unknown): Promise<T>;
  emit(event: string, payload?: unknown, groups?: string | string[]): Promise<void>;
  broadcast(event: string, payload?: unknown, groups?: string | string[]): Promise<void>;
  getLogger(module: string, service?: unknown): Logger;
  errorRegenerator?: {
    restore(err: Record<string, unknown>, stack?: string | Record<string, unknown>): Error;
    extractPlainError(err: Error): Errors.PlainMoleculerError;
  };
  ContextFactory?: {
    create(broker: ServiceBroker, endpoint: unknown, params: unknown, opts: unknown): Context;
  };
}
