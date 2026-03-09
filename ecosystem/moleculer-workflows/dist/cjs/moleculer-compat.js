"use strict";
/**
 * moleculer-compat.ts
 *
 * Lightweight stand-in for the `moleculer` Node.js package types.
 * Used by moleculer-workflows so it can run without the full moleculer package.
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.Utils = exports.Serializers = exports.JsonSerializer = exports.METRIC = exports.Errors = exports.BrokerOptionsError = exports.ValidationError = exports.ServiceSchemaError = exports.MoleculerClientError = exports.MoleculerRetryableError = exports.MoleculerError = void 0;
// ─── Errors ──────────────────────────────────────────────────────────────────
class MoleculerError extends Error {
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
exports.MoleculerError = MoleculerError;
class MoleculerRetryableError extends MoleculerError {
    retryable = true;
    constructor(message, code = 500, type = "MOLECULER_RETRYABLE_ERROR", data = null) {
        super(message, code, type, data);
        this.name = "MoleculerRetryableError";
    }
}
exports.MoleculerRetryableError = MoleculerRetryableError;
class MoleculerClientError extends MoleculerError {
    constructor(message, code = 422, type = "MOLECULER_CLIENT_ERROR", data = null) {
        super(message, code, type, data);
        this.name = "MoleculerClientError";
    }
}
exports.MoleculerClientError = MoleculerClientError;
class ServiceSchemaError extends MoleculerError {
    constructor(message, data = null) {
        super(message, 500, "SERVICE_SCHEMA_ERROR", data);
        this.name = "ServiceSchemaError";
    }
}
exports.ServiceSchemaError = ServiceSchemaError;
class ValidationError extends MoleculerClientError {
    constructor(message, type = "VALIDATION_ERROR", data = null) {
        super(message, 422, type, data);
        this.name = "ValidationError";
    }
}
exports.ValidationError = ValidationError;
class BrokerOptionsError extends MoleculerError {
    constructor(message, data = null) {
        super(message, 500, "BROKER_OPTIONS_ERROR", data);
        this.name = "BrokerOptionsError";
    }
}
exports.BrokerOptionsError = BrokerOptionsError;
exports.Errors = {
    MoleculerError, MoleculerRetryableError, MoleculerClientError,
    BrokerOptionsError, ServiceSchemaError, ValidationError,
};
// ─── Metrics constants ────────────────────────────────────────────────────────
exports.METRIC = {
    TYPE_COUNTER: "counter",
    TYPE_GAUGE: "gauge",
    TYPE_HISTOGRAM: "histogram",
    TYPE_INFO: "info",
};
// ─── Serializers ─────────────────────────────────────────────────────────────
class JsonSerializer {
    init(_broker) { }
    serialize(obj) { return Buffer.from(JSON.stringify(obj)); }
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    deserialize(buf) { return JSON.parse(buf.toString()); }
}
exports.JsonSerializer = JsonSerializer;
var Serializers;
(function (Serializers) {
    function resolve(type) {
        if (!type || type === "JSON")
            return new JsonSerializer();
        throw new ServiceSchemaError(`Unknown serializer: "${type}". Only JSON is supported.`);
    }
    Serializers.resolve = resolve;
})(Serializers || (exports.Serializers = Serializers = {}));
// ─── Utils ────────────────────────────────────────────────────────────────────
exports.Utils = {
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
