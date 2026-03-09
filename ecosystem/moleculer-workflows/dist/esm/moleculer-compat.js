/**
 * moleculer-compat.ts
 *
 * Lightweight stand-in for the `moleculer` Node.js package types.
 * Used by moleculer-workflows so it can run without the full moleculer package.
 */
// ─── Errors ──────────────────────────────────────────────────────────────────
export class MoleculerError extends Error {
    code;
    type;
    data;
    constructor(message, code = 500, type = "MOLECULER_ERROR", data = null) {
        super(message);
        this.name = "MoleculerError";
        this.code = code;
        this.type = type;
        this.data = data;
    }
}
export class MoleculerRetryableError extends MoleculerError {
    retryable = true;
    constructor(message, code = 500, type = "MOLECULER_RETRYABLE_ERROR", data = null) {
        super(message, code, type, data);
        this.name = "MoleculerRetryableError";
    }
}
export class MoleculerClientError extends MoleculerError {
    constructor(message, code = 422, type = "MOLECULER_CLIENT_ERROR", data = null) {
        super(message, code, type, data);
        this.name = "MoleculerClientError";
    }
}
export class ServiceSchemaError extends MoleculerError {
    constructor(message, data = null) {
        super(message, 500, "SERVICE_SCHEMA_ERROR", data);
        this.name = "ServiceSchemaError";
    }
}
export class ValidationError extends MoleculerClientError {
    constructor(message, type = "VALIDATION_ERROR", data = null) {
        super(message, 422, type, data);
        this.name = "ValidationError";
    }
}
export class BrokerOptionsError extends MoleculerError {
    constructor(message, data = null) {
        super(message, 500, "BROKER_OPTIONS_ERROR", data);
        this.name = "BrokerOptionsError";
    }
}
export const Errors = {
    MoleculerError, MoleculerRetryableError, MoleculerClientError,
    BrokerOptionsError, ServiceSchemaError, ValidationError,
};
// ─── Metrics constants ────────────────────────────────────────────────────────
export const METRIC = {
    TYPE_COUNTER: "counter",
    TYPE_GAUGE: "gauge",
    TYPE_HISTOGRAM: "histogram",
    TYPE_INFO: "info",
};
// ─── Serializers ─────────────────────────────────────────────────────────────
export class JsonSerializer {
    init(_broker) { }
    serialize(obj) { return Buffer.from(JSON.stringify(obj)); }
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    deserialize(buf) { return JSON.parse(buf.toString()); }
}
export var Serializers;
(function (Serializers) {
    function resolve(type) {
        if (!type || type === "JSON")
            return new JsonSerializer();
        throw new ServiceSchemaError(`Unknown serializer: "${type}". Only JSON is supported.`);
    }
    Serializers.resolve = resolve;
})(Serializers || (Serializers = {}));
// ─── Utils ────────────────────────────────────────────────────────────────────
export const Utils = {
    isFunction: (fn) => typeof fn === "function",
    isPlainObject: (obj) => obj !== null && typeof obj === "object" && !Array.isArray(obj),
    safetyObject: (obj) => {
        const SENSITIVE = new Set(["password", "passwd", "secret", "token", "apikey", "api_key"]);
        function mask(o) {
            if (o === null || typeof o !== "object" || Array.isArray(o))
                return o;
            return Object.fromEntries(Object.entries(o).map(([k, v]) => [k, SENSITIVE.has(k.toLowerCase()) ? "***" : mask(v)]));
        }
        return mask(obj);
    },
    makeDirs: (_path) => { },
};
