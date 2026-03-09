import type { ServiceBroker, Service, Context, Logger } from "./moleculer-compat.ts";
import type BaseAdapter from "./adapters/base.ts";
import type { WorkflowHandler, WorkflowsMiddlewareOptions, Job, JobEvent, CreateJobOptions } from "./types.ts";
export interface WorkflowOptions {
    name?: string;
    timeout?: string | number;
    retention?: string | number;
    concurrency?: number;
    retryPolicy?: {
        retries?: number;
        delay?: number;
        maxDelay?: number;
        factor?: number;
    };
    removeOnCompleted?: boolean;
    removeOnFailed?: boolean;
    params?: Record<string, unknown>;
    tracing?: boolean | {
        enabled?: boolean;
        tags?: Record<string, unknown> | (() => Record<string, unknown>);
        safetyTags?: boolean;
        spanName?: string | ((svc: Service, ctx: Context) => string);
    };
    maxStalledCount?: number;
}
export interface WorkflowSchema extends WorkflowOptions {
    enabled?: boolean;
    fullName?: string;
    handler: WorkflowHandler;
}
export default class Workflow {
    opts: WorkflowOptions;
    name: string | undefined;
    svc?: Service;
    handler: WorkflowHandler;
    adapter: BaseAdapter | null;
    activeJobs: string[];
    maintenanceTimer: NodeJS.Timeout | null;
    lastRetentionTime: number | null;
    delayedNextTime: number | null;
    delayedTimer: NodeJS.Timeout | null;
    broker: ServiceBroker;
    logger: Logger;
    mwOpts: WorkflowsMiddlewareOptions;
    /**
     * Constructor of workflow.
     *
     * @param schema Workflow schema containing the workflow options and handler.
     * @param svc
     */
    constructor(schema: WorkflowSchema, svc?: Service);
    /**
     * Initialize the workflow.
     *
     * @param broker
     * @param logger
     * @param mwOpts
     */
    init(broker: ServiceBroker, logger: Logger, mwOpts: WorkflowsMiddlewareOptions): void;
    /**
     * Log a message with the given level.
     *
     * @param level
     * @param jobId
     * @param msg
     * @param args
     */
    log(level: string, jobId: string | null, msg: string, ...args: unknown[]): void;
    /**
     * Start the workflow.
     */
    start(): Promise<void>;
    /**
     * Called after the adapter is connected.
     * Starts the job processor and sets the next maintenance time.
     */
    afterAdapterConnected(): Promise<void>;
    /**
     * Stop the workflow.
     */
    stop(): Promise<void>;
    addRunningJob(jobId: string): void;
    removeRunningJob(jobId: string): void;
    getNumberOfActiveJobs(): number;
    metricsIncrement(metricName: string): void;
    /**
     * Create a workflow context for the given workflow and job.
     *
     * @param job The job object.
     * @param events The list of events associated with the job.
     * @returns The created workflow context.
     */
    createWorkflowContext(job: Job, events: JobEvent[]): Context;
    /**
     * Call workflow handler with a job.
     *
     * @param job
     * @param events
     * @returns
     */
    callHandler(job: Job, events: JobEvent[]): Promise<unknown>;
    getRoundedNextTime(time: number): number;
    /**
     * Calculate the next maintenance time. We use 'circa' to randomize the time a bit
     * and avoid that all adapters run the maintenance at the same time.
     */
    setNextMaintenance(): void;
    /**
     * Run the maintenance tasks.
     */
    maintenance(): Promise<void>;
    /**
     * Run the delayed jobs maintenance tasks.
     */
    maintenanceDelayed(): Promise<void>;
    /**
     * Set the next delayed jobs maintenance timer for a workflow.
     *
     * @param nextTime Optional timestamp to schedule next maintenance.
     */
    setNextDelayedMaintenance(nextTime?: number): Promise<void>;
    /**
     * Check if the workflow name is valid.
     *
     * @param workflowName
     */
    static checkWorkflowName(workflowName: string): string;
    /**
     * Check if the job ID is valid.
     *
     * @param signalName
     * @param key
     */
    static checkSignal(signalName: string, key: string | null): void;
    /**
     * Create a new job and push it to the waiting or delayed queue.
     *
     * @param adapter - The adapter instance.
     * @param workflowName - The name of the workflow.
     * @param payload - The job payload.
     * @param opts - Additional options for the job.
     * @returns Resolves with the created job object.
     */
    static createJob(adapter: BaseAdapter, workflowName: string, payload: unknown, opts: CreateJobOptions): Promise<Job>;
    /**
     * Reschedule a repeatable job based on its configuration.
     *
     * @param adapter - The adapter instance.
     * @param workflowName - The name of workflow.
     * @param job - The job object or job ID to reschedule.
     * @returns Resolves when the job is rescheduled.
     */
    static rescheduleJob(adapter: BaseAdapter, workflowName: string, job: Job | string): Promise<void>;
}
//# sourceMappingURL=workflow.d.ts.map