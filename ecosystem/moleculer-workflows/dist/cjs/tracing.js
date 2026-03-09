"use strict";
/*
 * @moleculer/workflows
 * Copyright (c) 2025 MoleculerJS (https://github.com/moleculerjs/workflows)
 * MIT Licensed
 */
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
exports.default = tracingLocalChannelMiddleware;
const lodash_1 = __importDefault(require("lodash"));
const moleculer_compat_ts_1 = require("./moleculer-compat.js");
const { isFunction, isPlainObject, safetyObject } = moleculer_compat_ts_1.Utils;
function tracingLocalChannelMiddleware(handler, wf) {
    let opts;
    if (wf.tracing === true || wf.tracing === false)
        opts = { enabled: !!wf.tracing };
    else
        opts = lodash_1.default.defaultsDeep({}, opts, { enabled: true });
    // eslint-disable-next-line @typescript-eslint/no-this-alias
    const broker = this;
    const tracer = this.tracer;
    if (broker.isTracingEnabled() && opts.enabled) {
        return function tracingLocalChannelMiddleware(ctx) {
            ctx.requestID = ctx.requestID || tracer.getCurrentTraceID();
            ctx.parentID = ctx.parentID || tracer.getActiveSpanID();
            let tags = {
                callingLevel: ctx.level,
                workflow: {
                    name: ctx.wf.name,
                    jobId: ctx.wf.jobId
                },
                remoteCall: ctx.nodeID !== broker.nodeID,
                callerNodeID: ctx.nodeID,
                nodeID: broker.nodeID,
                /*options: {
                        timeout: ctx.options.timeout,
                        retries: ctx.options.retries
                    },*/
                requestID: ctx.requestID
            };
            let actionTags;
            // local action tags take precedence
            if (isFunction(opts.tags)) {
                actionTags = opts.tags();
            }
            else {
                // By default all params are captured. This can be overridden globally and locally
                actionTags = { ...{ params: true }, ...opts.tags };
            }
            if (isFunction(actionTags)) {
                const res = actionTags.call(ctx.service, ctx);
                if (res)
                    Object.assign(tags, res);
            }
            else if (isPlainObject(actionTags)) {
                if (actionTags.params === true)
                    tags.params =
                        ctx.params != null && isPlainObject(ctx.params)
                            ? Object.assign({}, ctx.params)
                            : ctx.params;
                else if (Array.isArray(actionTags.params))
                    tags.params = lodash_1.default.pick(ctx.params, actionTags.params);
                if (actionTags.meta === true)
                    tags.meta = ctx.meta != null ? Object.assign({}, ctx.meta) : ctx.meta;
                else if (Array.isArray(actionTags.meta))
                    tags.meta = lodash_1.default.pick(ctx.meta, actionTags.meta);
            }
            if (opts.safetyTags) {
                tags = safetyObject(tags);
            }
            let spanName = `workflow '${wf.name}'`;
            if (opts.spanName) {
                switch (typeof opts.spanName) {
                    case "string":
                        spanName = opts.spanName;
                        break;
                    case "function":
                        spanName = opts.spanName.call(ctx.service, ctx);
                        break;
                }
            }
            const span = ctx.startSpan(spanName, {
                id: ctx.id,
                type: "workflow",
                traceID: ctx.requestID,
                parentID: ctx.parentID,
                service: ctx.service,
                sampled: ctx.tracing,
                tags
            });
            ctx.tracing = span.sampled;
            // Call the handler
            return handler(ctx)
                .then(res => {
                ctx.finishSpan(span);
                return res;
            })
                .catch(err => {
                span.setError(err);
                ctx.finishSpan(span);
                throw err;
            });
        }.bind(this);
    }
    return handler;
}
