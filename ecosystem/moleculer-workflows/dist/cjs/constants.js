"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.METRIC_WORKFLOWS_SIGNALS_TOTAL = exports.METRIC_WORKFLOWS_JOBS_RETRIES_TOTAL = exports.METRIC_WORKFLOWS_JOBS_ERRORS_TOTAL = exports.METRIC_WORKFLOWS_JOBS_TIME = exports.METRIC_WORKFLOWS_JOBS_ACTIVE = exports.METRIC_WORKFLOWS_JOBS_TOTAL = exports.METRIC_WORKFLOWS_JOBS_CREATED = exports.FINISHED = exports.QUEUE_FAILED = exports.QUEUE_COMPLETED = exports.QUEUE_DELAYED = exports.QUEUE_STALLED = exports.QUEUE_ACTIVE = exports.QUEUE_WAITING = exports.QUEUE_MAINTENANCE_LOCK_DELAYED = exports.QUEUE_MAINTENANCE_LOCK = exports.QUEUE_JOB_EVENTS = exports.QUEUE_JOB_LOCK = exports.QUEUE_JOB = exports.QUEUE_CATEGORY_SIGNAL = exports.QUEUE_CATEGORY_WF = exports.RERUN_REMOVABLE_FIELDS = exports.JOB_FIELDS_BOOLEAN = exports.JOB_FIELDS_NUMERIC = exports.JOB_FIELDS_JSON = exports.SIGNAL_EMPTY_KEY = void 0;
exports.SIGNAL_EMPTY_KEY = "null";
exports.JOB_FIELDS_JSON = ["payload", "repeat", "result", "error", "state"];
exports.JOB_FIELDS_NUMERIC = [
    "createdAt",
    "startedAt",
    "finishedAt",
    "promoteAt",
    "repeatCounter",
    "stalledCounter",
    "delay",
    "timeout",
    "duration",
    "retries",
    "retryAttempts"
];
exports.JOB_FIELDS_BOOLEAN = ["success"];
// export const JOB_FIELDS_STRING: string[] = ["id", "parent", "nodeID"];
// export const JOB_FIELDS: string[] = [...JOB_FIELDS_STRING, ...JOB_FIELDS_JSON, ...JOB_FIELDS_NUMERIC, ...JOB_FIELDS_BOOLEAN];
exports.RERUN_REMOVABLE_FIELDS = [
    "startedAt",
    "duration",
    "finishedAt",
    "nodeID",
    "success",
    "error",
    "result"
];
exports.QUEUE_CATEGORY_WF = "workflows";
exports.QUEUE_CATEGORY_SIGNAL = "signals";
exports.QUEUE_JOB = "job";
exports.QUEUE_JOB_LOCK = "job-lock";
exports.QUEUE_JOB_EVENTS = "job-events";
exports.QUEUE_MAINTENANCE_LOCK = "maintenance-lock";
exports.QUEUE_MAINTENANCE_LOCK_DELAYED = "maintenance-lock-delayed";
exports.QUEUE_WAITING = "waiting";
exports.QUEUE_ACTIVE = "active";
exports.QUEUE_STALLED = "stalled";
exports.QUEUE_DELAYED = "delayed";
exports.QUEUE_COMPLETED = "completed";
exports.QUEUE_FAILED = "failed";
exports.FINISHED = "finished";
exports.METRIC_WORKFLOWS_JOBS_CREATED = "moleculer.workflows.jobs.created";
exports.METRIC_WORKFLOWS_JOBS_TOTAL = "moleculer.workflows.jobs.total";
exports.METRIC_WORKFLOWS_JOBS_ACTIVE = "moleculer.workflows.jobs.active";
exports.METRIC_WORKFLOWS_JOBS_TIME = "moleculer.workflows.jobs.time";
exports.METRIC_WORKFLOWS_JOBS_ERRORS_TOTAL = "moleculer.workflows.jobs.errors.total";
exports.METRIC_WORKFLOWS_JOBS_RETRIES_TOTAL = "moleculer.workflows.jobs.retries.total";
exports.METRIC_WORKFLOWS_SIGNALS_TOTAL = "moleculer.workflows.signals.total";
