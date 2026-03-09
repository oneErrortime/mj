declare const MoleculerError: typeof import("./moleculer-compat.ts").MoleculerError, MoleculerRetryableError: typeof import("./moleculer-compat.ts").MoleculerRetryableError;
export declare class WorkflowError extends MoleculerError {
    constructor(message: string, code?: number, type?: string, data?: object);
}
export declare class WorkflowRetryableError extends MoleculerRetryableError {
    constructor(message: string, code?: number, type?: string, data?: object);
}
export declare class WorkflowTimeoutError extends WorkflowRetryableError {
    constructor(workflow: string, jobId: string, timeout: number);
}
export declare class WorkflowSignalTimeoutError extends WorkflowRetryableError {
    constructor(signal: string, key: string, timeout: number | string);
}
export declare class WorkflowTaskMismatchError extends WorkflowError {
    constructor(taskId: number, expected: string, actual: string);
}
export declare class WorkflowAlreadyLocked extends WorkflowRetryableError {
    constructor(jobId: string);
}
export declare class WorkflowMaximumStalled extends WorkflowRetryableError {
    constructor(jobId: string, maxStalledCount: number);
}
export {};
//# sourceMappingURL=errors.d.ts.map