/*
 * @moleculer/workflows
 * Copyright (c) 2025 MoleculerJS (https://github.com/moleculerjs/workflows)
 * MIT Licensed
 */
import _ from "lodash";
import { WorkflowError } from "../errors.js";
/**
 * Base adapter class
 */
export default class BaseAdapter {
    opts;
    connected;
    wf;
    broker;
    logger;
    mwOpts;
    /**
     * Constructor of adapter
     * @param opts
     */
    constructor(opts) {
        this.opts = _.defaultsDeep({}, opts, {});
        this.connected = false;
    }
    /**
     * Initialize the adapter.
     *
     * @param wf
     * @param broker
     * @param logger
     * @param mwOpts - Middleware options.
     */
    init(wf, broker, logger, mwOpts) {
        /** @type {Workflow} */
        this.wf = wf;
        this.broker = broker;
        this.logger = logger;
        this.mwOpts = mwOpts;
    }
    /**
     * Log a message with the given level.
     *
     * @param level
     * @param workflowName
     * @param jobId
     * @param msg
     * @param args
     */
    log(level, workflowName, jobId, msg, ...args) {
        if (this.logger) {
            const wfJobName = jobId ? `${workflowName}:${jobId}` : workflowName;
            this.logger[level](`[${wfJobName}] ${msg}`, ...args);
        }
    }
    /**
     * Send entity lifecycle events
     *
     * @param workflowName
     * @param jobId
     * @param type
     */
    sendJobEvent(workflowName, jobId, type) {
        if (this.mwOpts?.jobEventType) {
            const eventName = `job.${workflowName}.${type}`;
            const payload = {
                type,
                workflow: workflowName,
                job: jobId
            };
            this.broker[this.mwOpts.jobEventType](eventName, payload);
        }
    }
    /**
     * Check if the job ID is valid.
     *
     * @param jobId
     */
    checkJobId(jobId) {
        const re = /^[a-zA-Z0-9_.-]+$/;
        if (!re.test(jobId)) {
            throw new WorkflowError(`Invalid job ID '${jobId}'. Only alphanumeric characters, underscore, dot and dash are allowed.`, 400, "INVALID_JOB_ID", {
                jobId
            });
        }
        return jobId;
    }
}
