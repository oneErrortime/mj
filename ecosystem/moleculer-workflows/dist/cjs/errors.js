/*
 * @moleculer/workflows
 * Copyright (c) 2025 MoleculerJS (https://github.com/moleculerjs/workflows)
 * MIT Licensed
 */
"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.WorkflowMaximumStalled = exports.WorkflowAlreadyLocked = exports.WorkflowTaskMismatchError = exports.WorkflowSignalTimeoutError = exports.WorkflowTimeoutError = exports.WorkflowRetryableError = exports.WorkflowError = void 0;
const moleculer_compat_ts_1 = require("./moleculer-compat.js");
const { MoleculerError, MoleculerRetryableError } = moleculer_compat_ts_1.Errors;
class WorkflowError extends MoleculerError {
    constructor(message, code, type, data) {
        super(message, code ?? 500, type ?? "WORKFLOW_ERROR", data);
    }
}
exports.WorkflowError = WorkflowError;
class WorkflowRetryableError extends MoleculerRetryableError {
    constructor(message, code, type, data) {
        super(message, code ?? 500, type ?? "WORKFLOW_ERROR", data);
    }
}
exports.WorkflowRetryableError = WorkflowRetryableError;
class WorkflowTimeoutError extends WorkflowRetryableError {
    constructor(workflow, jobId, timeout) {
        super("Job timed out", 500, "WORKFLOW_JOB_TIMEOUT", { workflow, jobId, timeout });
    }
}
exports.WorkflowTimeoutError = WorkflowTimeoutError;
class WorkflowSignalTimeoutError extends WorkflowRetryableError {
    constructor(signal, key, timeout) {
        super("Signal timed out", 500, "WORKFLOW_SIGNAL_TIMEOUT", { signal, key, timeout });
    }
}
exports.WorkflowSignalTimeoutError = WorkflowSignalTimeoutError;
class WorkflowTaskMismatchError extends WorkflowError {
    constructor(taskId, expected, actual) {
        super(`Workflow task mismatch at replaying. Expected '${expected}' but got '${actual}'.`, 500, "WORKFLOW_TASK_MISMATCH", {
            taskId,
            expected,
            actual
        });
    }
}
exports.WorkflowTaskMismatchError = WorkflowTaskMismatchError;
class WorkflowAlreadyLocked extends WorkflowRetryableError {
    constructor(jobId) {
        super("Job is already locked", 500, "WORKFLOW_ALREADY_LOCKED", { jobId });
    }
}
exports.WorkflowAlreadyLocked = WorkflowAlreadyLocked;
class WorkflowMaximumStalled extends WorkflowRetryableError {
    constructor(jobId, maxStalledCount) {
        super("Job stalled too many times.", 500, "WORKFLOW_MAXIMUM_STALLED", {
            jobId,
            maxStalledCount
        });
    }
}
exports.WorkflowMaximumStalled = WorkflowMaximumStalled;
