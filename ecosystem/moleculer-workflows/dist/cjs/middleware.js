"use strict";
/*
 * @moleculer/workflows
 * Copyright (c) 2025 MoleculerJS (https://github.com/moleculerjs/workflows)
 * MIT Licensed
 */
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || (function () {
    var ownKeys = function(o) {
        ownKeys = Object.getOwnPropertyNames || function (o) {
            var ar = [];
            for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
            return ar;
        };
        return ownKeys(o);
    };
    return function (mod) {
        if (mod && mod.__esModule) return mod;
        var result = {};
        if (mod != null) for (var k = ownKeys(mod), i = 0; i < k.length; i++) if (k[i] !== "default") __createBinding(result, mod, k[i]);
        __setModuleDefault(result, mod);
        return result;
    };
})();
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
exports.default = WorkflowsMiddleware;
const lodash_1 = __importDefault(require("lodash"));
const moleculer_compat_ts_1 = require("./moleculer-compat.js");
const workflow_ts_1 = __importDefault(require("./workflow.js"));
const index_ts_1 = __importDefault(require("./adapters/index.js"));
const C = __importStar(require("./constants.js"));
const tracing_ts_1 = __importDefault(require("./tracing.js"));
/**
 * WorkflowsMiddleware for Moleculer
 */
function WorkflowsMiddleware(mwOpts) {
    mwOpts = lodash_1.default.defaultsDeep({}, mwOpts, {
        adapter: "Redis",
        schemaProperty: "workflows",
        workflowHandlerTrigger: "emitLocalWorkflowHandler",
        jobEventType: null,
        signalExpiration: "1h",
        maintenanceTime: 10,
        lockExpiration: 30,
        jobIdCollision: "reject"
    });
    let broker;
    let logger;
    let adapter;
    /**
     * Register metrics for workflows
     */
    function registerMetrics(broker) {
        if (!broker.isMetricsEnabled())
            return;
        broker.metrics.register({
            type: moleculer_compat_ts_1.METRIC.TYPE_COUNTER,
            name: C.METRIC_WORKFLOWS_JOBS_CREATED,
            labelNames: ["workflow"],
            rate: true,
            unit: "job"
        });
        broker.metrics.register({
            type: moleculer_compat_ts_1.METRIC.TYPE_COUNTER,
            name: C.METRIC_WORKFLOWS_JOBS_TOTAL,
            labelNames: ["workflow"],
            rate: true,
            unit: "job"
        });
        broker.metrics.register({
            type: moleculer_compat_ts_1.METRIC.TYPE_GAUGE,
            name: C.METRIC_WORKFLOWS_JOBS_ACTIVE,
            labelNames: ["workflow"],
            rate: true,
            unit: "job"
        });
        broker.metrics.register({
            type: moleculer_compat_ts_1.METRIC.TYPE_HISTOGRAM,
            name: C.METRIC_WORKFLOWS_JOBS_TIME,
            labelNames: ["workflow"],
            quantiles: true,
            unit: "job"
        });
        broker.metrics.register({
            type: moleculer_compat_ts_1.METRIC.TYPE_GAUGE,
            name: C.METRIC_WORKFLOWS_JOBS_ERRORS_TOTAL,
            labelNames: ["workflow"],
            rate: true,
            unit: "job"
        });
        broker.metrics.register({
            type: moleculer_compat_ts_1.METRIC.TYPE_GAUGE,
            name: C.METRIC_WORKFLOWS_JOBS_RETRIES_TOTAL,
            labelNames: ["workflow"],
            rate: true,
            unit: "job"
        });
        broker.metrics.register({
            type: moleculer_compat_ts_1.METRIC.TYPE_GAUGE,
            name: C.METRIC_WORKFLOWS_SIGNALS_TOTAL,
            labelNames: ["signal"],
            rate: true,
            unit: "signal"
        });
    }
    const middleware = {
        name: "Workflows",
        /**
         * Created lifecycle hook of ServiceBroker
         */
        created(_broker) {
            broker = _broker;
            logger = broker.getLogger("Workflows");
            // Populate broker with new methods
            if (!broker.wf) {
                broker.wf = {};
            }
            broker.wf.getAdapter = async () => {
                if (!adapter) {
                    adapter = index_ts_1.default.resolve(mwOpts.adapter);
                    adapter.init(null, broker, logger, mwOpts);
                    await adapter.connect();
                }
                return adapter;
            };
            /**
             * Execute a workflow
             *
             * @param workflowName
             * @param payload
             * @param opts
             */
            broker.wf.run = async (workflowName, payload, opts) => {
                workflow_ts_1.default.checkWorkflowName(workflowName);
                if (broker.isMetricsEnabled()) {
                    broker.metrics.increment(C.METRIC_WORKFLOWS_JOBS_CREATED, {
                        workflow: workflowName
                    });
                }
                return workflow_ts_1.default.createJob(await broker.wf.getAdapter(), workflowName, payload, opts);
            };
            /**
             * Remove a workflow job
             *
             * @param workflowName
             * @param jobId
             * @returns
             */
            broker.wf.remove = async (workflowName, jobId) => {
                workflow_ts_1.default.checkWorkflowName(workflowName);
                if (!jobId) {
                    return Promise.reject(new moleculer_compat_ts_1.Errors.MoleculerError("Job ID is required!", 400, "JOB_ID_REQUIRED"));
                }
                return (await broker.wf.getAdapter()).cleanUp(workflowName, jobId);
            };
            /**
             * Trigger a named signal.
             *
             * @param signalName
             * @param key
             * @param payload
             * @returns
             */
            broker.wf.triggerSignal = async (signalName, key, payload) => {
                if (!signalName) {
                    return Promise.reject(new moleculer_compat_ts_1.Errors.MoleculerError("Signal name is required!", 400, "SIGNAL_NAME_REQUIRED"));
                }
                workflow_ts_1.default.checkSignal(signalName, key);
                if (broker.isMetricsEnabled()) {
                    broker.metrics.increment(C.METRIC_WORKFLOWS_SIGNALS_TOTAL, {
                        signal: signalName
                    });
                }
                return (await broker.wf.getAdapter()).triggerSignal(signalName, key, payload);
            };
            /**
             * Remove a named signal.
             *
             * @param signalName
             * @param key
             * @returns
             */
            broker.wf.removeSignal = async (signalName, key) => {
                if (!signalName) {
                    return Promise.reject(new moleculer_compat_ts_1.Errors.MoleculerError("Signal name is required!", 400, "SIGNAL_NAME_REQUIRED"));
                }
                workflow_ts_1.default.checkSignal(signalName, key);
                return (await broker.wf.getAdapter()).removeSignal(signalName, key);
            };
            /**
             * Get state of a workflow run.
             *
             * @param workflowName
             * @param jobId
             * @returns
             */
            broker.wf.getState = async (workflowName, jobId) => {
                workflow_ts_1.default.checkWorkflowName(workflowName);
                if (!jobId) {
                    return Promise.reject(new moleculer_compat_ts_1.Errors.MoleculerError("Job ID is required!", 400, "JOB_ID_REQUIRED"));
                }
                return (await broker.wf.getAdapter()).getState(workflowName, jobId);
            };
            /**
             * Get job details of a workflow run.
             *
             * @param workflowName
             * @param jobId
             * @returns
             */
            broker.wf.get = async (workflowName, jobId) => {
                workflow_ts_1.default.checkWorkflowName(workflowName);
                if (!jobId) {
                    return Promise.reject(new moleculer_compat_ts_1.Errors.MoleculerError("Job ID is required!", 400, "JOB_ID_REQUIRED"));
                }
                return (await broker.wf.getAdapter()).getJob(workflowName, jobId, true);
            };
            /**
             * Get job events of a workflow run.
             *
             * @param workflowName
             * @param jobId
             * @returns
             */
            broker.wf.getEvents = async (workflowName, jobId) => {
                workflow_ts_1.default.checkWorkflowName(workflowName);
                if (!jobId) {
                    return Promise.reject(new moleculer_compat_ts_1.Errors.MoleculerError("Job ID is required!", 400, "JOB_ID_REQUIRED"));
                }
                return (await broker.wf.getAdapter()).getJobEvents(workflowName, jobId);
            };
            /**
             * List completed jobs for a workflow.
             *
             * @param {string} workflowName
             * @returns
             */
            broker.wf.listCompletedJobs = async (workflowName) => {
                workflow_ts_1.default.checkWorkflowName(workflowName);
                return (await broker.wf.getAdapter()).listCompletedJobs(workflowName);
            };
            /**
             * List failed jobs for a workflow.
             *
             * @param {string} workflowName
             * @returns
             */
            broker.wf.listFailedJobs = async (workflowName) => {
                workflow_ts_1.default.checkWorkflowName(workflowName);
                return (await broker.wf.getAdapter()).listFailedJobs(workflowName);
            };
            /**
             * List delayed jobs for a workflow.
             *
             * @param {string} workflowName
             * @returns
             */
            broker.wf.listDelayedJobs = async (workflowName) => {
                workflow_ts_1.default.checkWorkflowName(workflowName);
                return (await broker.wf.getAdapter()).listDelayedJobs(workflowName);
            };
            /**
             * List active jobs for a workflow.
             *
             * @param {string} workflowName
             * @returns
             */
            broker.wf.listActiveJobs = async (workflowName) => {
                workflow_ts_1.default.checkWorkflowName(workflowName);
                return (await broker.wf.getAdapter()).listActiveJobs(workflowName);
            };
            /**
             * List waiting jobs for a workflow.
             *
             * @param {string} workflowName
             * @returns
             */
            broker.wf.listWaitingJobs = async (workflowName) => {
                workflow_ts_1.default.checkWorkflowName(workflowName);
                return (await broker.wf.getAdapter()).listWaitingJobs(workflowName);
            };
            /**
             * Delete all workflow jobs & history.
             *
             * @param {string} workflowName
             * @returns
             */
            broker.wf.cleanUp = async (workflowName) => {
                workflow_ts_1.default.checkWorkflowName(workflowName);
                return (await broker.wf.getAdapter()).cleanUp(workflowName);
            };
            registerMetrics(broker);
        },
        /**
         * Created lifecycle hook of service
         */
        async serviceCreated(svc) {
            if (lodash_1.default.isPlainObject(svc.schema[mwOpts.schemaProperty])) {
                svc.$workflows = [];
                for (const [name, def] of Object.entries(svc.schema[mwOpts.schemaProperty])) {
                    let wf;
                    if (lodash_1.default.isFunction(def)) {
                        wf = { handler: def };
                    }
                    else if (lodash_1.default.isPlainObject(def)) {
                        wf = lodash_1.default.cloneDeep(def);
                    }
                    else {
                        throw new moleculer_compat_ts_1.Errors.ServiceSchemaError(`Invalid workflow definition in '${name}' workflow in '${svc.fullName}' service!`, svc.schema);
                    }
                    if (wf.enabled === false)
                        continue;
                    if (!lodash_1.default.isFunction(wf.handler)) {
                        throw new moleculer_compat_ts_1.Errors.ServiceSchemaError(`Missing workflow handler on '${name}' workflow in '${svc.fullName}' service!`, svc.schema);
                    }
                    wf.name = wf.fullName ? wf.fullName : svc.fullName + "." + (wf.name || name);
                    workflow_ts_1.default.checkWorkflowName(wf.name);
                    // Wrap the original handler
                    const handler = wf.handler.bind(svc);
                    // Wrap the handler with custom middlewares
                    const handler2 = broker.middlewares.wrapHandler("localWorkflow", handler, wf);
                    wf.handler = handler2;
                    // Add metrics for the handler
                    if (broker.isMetricsEnabled()) {
                        wf.handler = async (ctx) => {
                            const labels = { workflow: wf.name };
                            const timeEnd = broker.metrics.timer(C.METRIC_WORKFLOWS_JOBS_TIME, labels);
                            broker.metrics.increment(C.METRIC_WORKFLOWS_JOBS_ACTIVE, labels);
                            try {
                                const result = await handler2(ctx);
                                return result;
                            }
                            catch (err) {
                                broker.metrics.increment(C.METRIC_WORKFLOWS_JOBS_ERRORS_TOTAL, labels);
                                throw err;
                            }
                            finally {
                                timeEnd();
                                broker.metrics.decrement(C.METRIC_WORKFLOWS_JOBS_ACTIVE, labels);
                                broker.metrics.increment(C.METRIC_WORKFLOWS_JOBS_TOTAL, labels);
                            }
                        };
                    }
                    if (wf.params && broker.validator) {
                        const handler3 = wf.handler;
                        const check = broker.validator.compile(wf.params);
                        wf.handler = async (ctx) => {
                            const res = await check(ctx.params != null ? ctx.params : {});
                            if (res === true)
                                return handler3(ctx);
                            else {
                                throw new moleculer_compat_ts_1.Errors.ValidationError("Parameters validation error!", null, res);
                            }
                        };
                    }
                    const workflow = new workflow_ts_1.default(wf, svc);
                    await workflow.init(broker, logger, mwOpts);
                    // Register the workflow handler into the adapter
                    svc.$workflows.push(workflow);
                    logger.info(`Workflow '${workflow.name}' is registered.`);
                }
                /**
                 * Call a local channel event handler. Useful for unit tests.
                 * TODO:
                 *
                 * @param {String} workflowName
                 * @param {Object} payload
                 * @param {string?} jobId
                 * @returns
                 *
                svc[mwOpts.channelHandlerTrigger] = (workflowName, payload, jobId) => {
                    if (!jobId) {
                        jobId = broker.generateUid();
                    }

                    svc.logger.debug(
                        `${mwOpts.channelHandlerTrigger} called '${workflowName}' workflow handler`
                    );

                    if (!svc.schema[mwOpts.schemaProperty][workflowName]) {
                        return Promise.reject(
                            new MoleculerError(
                                `'${workflowName}' is not registered as local workflow event handler`,
                                500,
                                "NOT_FOUND_WORKFLOW",
                                { workflowName }
                            )
                        );
                    }

                    const ctx = (await broker.wf.getAdapter()).createWorkflowContext(workflow, job, events);

                    // Shorthand definition
                    if (typeof svc.schema[mwOpts.schemaProperty][workflowName] === "function")
                        return svc.schema[mwOpts.schemaProperty][workflowName].call(
                            svc, // Attach reference to service
                            ctx
                        );

                    // Object definition
                    return svc.schema[mwOpts.schemaProperty][workflowName].handler.call(
                        svc, // Attach reference to service
                        ctx
                    );
                };
                */
            }
        },
        /**
         * Service started lifecycle hook.
         * Need to register workflows.
         */
        async serviceStarted(svc) {
            if (!svc.$workflows)
                return;
            for (const wf of svc.$workflows) {
                await wf.start();
            }
        },
        /**
         * Service stopping lifecycle hook.
         * Need to unregister workflows.
         */
        async serviceStopping(svc) {
            if (!svc.$workflows)
                return;
            for (const wf of svc.$workflows) {
                await wf.stop();
            }
        },
        /**
         * Start lifecycle hook of ServiceBroker
         */
        async started() { },
        /**
         * Stop lifecycle hook of ServiceBroker
         */
        async stopped() {
            if (adapter) {
                await adapter.disconnect();
                adapter = null;
            }
        },
        localWorkflow: null
    };
    if (mwOpts.tracing) {
        middleware.localWorkflow = tracing_ts_1.default;
    }
    return middleware;
}
