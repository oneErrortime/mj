/**
 * moleculer-compat.ts
 *
 * Lightweight stand-in for the `moleculer` Node.js package types.
 * Used by moleculer-workflows so it can run without the full moleculer package.
 */
export declare class MoleculerError extends Error {
    code: number;
    type: string;
    data: unknown;
    constructor(message: string, code?: number, type?: string, data?: unknown);
}
export declare class MoleculerRetryableError extends MoleculerError {
    retryable: boolean;
    constructor(message: string, code?: number, type?: string, data?: unknown);
}
export declare class MoleculerClientError extends MoleculerError {
    constructor(message: string, code?: number, type?: string, data?: unknown);
}
export declare class ServiceSchemaError extends MoleculerError {
    constructor(message: string, data?: unknown);
}
export declare class ValidationError extends MoleculerClientError {
    constructor(message: string, type?: string, data?: unknown);
}
export declare class BrokerOptionsError extends MoleculerError {
    constructor(message: string, data?: unknown);
}
export declare const Errors: {
    MoleculerError: typeof MoleculerError;
    MoleculerRetryableError: typeof MoleculerRetryableError;
    MoleculerClientError: typeof MoleculerClientError;
    BrokerOptionsError: typeof BrokerOptionsError;
    ServiceSchemaError: typeof ServiceSchemaError;
    ValidationError: typeof ValidationError;
};
export declare namespace Errors {
    type PlainMoleculerError = {
        name: string;
        message: string;
        code?: number;
        type?: string;
        data?: unknown;
        stack?: string;
    };
}
export declare const METRIC: {
    readonly TYPE_COUNTER: "counter";
    readonly TYPE_GAUGE: "gauge";
    readonly TYPE_HISTOGRAM: "histogram";
    readonly TYPE_INFO: "info";
};
export declare class JsonSerializer {
    init(_broker?: unknown): void;
    serialize(obj: unknown): Buffer;
    deserialize(buf: Buffer): any;
}
export declare namespace Serializers {
    function resolve(type?: string): JsonSerializer;
    type Base = JsonSerializer;
}
export declare const Utils: {
    isFunction: (fn: unknown) => fn is Function;
    isPlainObject: (obj: unknown) => obj is Record<string, unknown>;
    safetyObject: (obj: unknown) => Record<string, unknown>;
    makeDirs: (_path: string) => void;
};
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
    validator?: {
        compile(schema: unknown): (obj: unknown) => boolean | unknown[];
    };
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
export declare const ServiceBrokerClass: new (opts?: Record<string, unknown>) => ServiceBroker;
//# sourceMappingURL=moleculer-compat.d.ts.map