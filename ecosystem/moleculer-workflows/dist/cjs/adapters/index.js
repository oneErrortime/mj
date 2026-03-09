/*
 * @moleculer/workflows
 * Copyright (c) 2025 MoleculerJS (https://github.com/moleculerjs/workflows)
 * MIT Licensed
 */
"use strict";
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const lodash_1 = __importDefault(require("lodash"));
const moleculer_compat_ts_1 = require("../moleculer-compat.js");
const base_ts_1 = __importDefault(require("./base.js"));
const redis_ts_1 = __importDefault(require("./redis.js"));
const Adapters = {
    Base: base_ts_1.default,
    // Fake: require("./fake"),
    Redis: redis_ts_1.default
};
function getByName(name) {
    if (!name)
        return null;
    const n = Object.keys(Adapters).find(n => n.toLowerCase() == name.toLowerCase());
    if (n)
        return Adapters[n];
}
/**
 * Resolve adapter by name
 *
 * @param opt
 */
function resolve(opt) {
    if (opt instanceof base_ts_1.default) {
        return opt;
    }
    else if (lodash_1.default.isString(opt)) {
        const AdapterClass = getByName(opt);
        if (AdapterClass) {
            // @ts-expect-error Solve it later
            return new AdapterClass();
        }
        else if (opt.startsWith("redis://") || opt.startsWith("rediss://")) {
            return new Adapters.Redis(opt);
        }
        else {
            throw new moleculer_compat_ts_1.Errors.ServiceSchemaError(`Invalid Adapter type '${opt}'.`, { type: opt });
        }
    }
    else if (lodash_1.default.isObject(opt)) {
        let AdapterClass = null;
        if (opt.type instanceof base_ts_1.default) {
            AdapterClass = opt.type;
        }
        else if (typeof opt.type === "string") {
            AdapterClass = getByName(opt.type || "Redis");
        }
        else {
            AdapterClass = getByName("Redis");
        }
        if (AdapterClass) {
            return new AdapterClass(opt.options);
        }
        else {
            throw new moleculer_compat_ts_1.Errors.ServiceSchemaError(`Invalid Adapter type '${opt.type}'.`, {
                type: opt.type
            });
        }
    }
    return new Adapters.Redis();
}
/**
 * Register a new Channel Adapter
 *
 * @param name
 * @param value
 */
function register(name, value) {
    Adapters[name] = value;
}
exports.default = Object.assign(Adapters, { resolve, register });
