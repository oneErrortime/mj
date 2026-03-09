import { Cluster, ClusterOptions, Redis as RedisClient, RedisOptions } from "ioredis";
import { Serializers, ServiceBroker, Logger } from "../moleculer-compat.ts";
import BaseAdapter, { ListJobResult, ListDelayedJobResult, ListFinishedJobResult } from "./base.ts";
import Workflow from "../workflow.ts";
import type { BaseDefaultOptions } from "./base.ts";
import { CreateJobOptions, Job, JobEvent, WorkflowsMiddlewareOptions, SignalWaitOptions } from "../types.ts";
export interface RedisAdapterOptions extends BaseDefaultOptions {
    url?: string;
    redis?: RedisOptions;
    cluster?: {
        nodes: string[];
        clusterOptions?: ClusterOptions;
    };
    prefix?: string;
    serializer?: string;
    drainDelay?: number;
}
export type StoredPromise<T = unknown> = {
    promise: Promise<T>;
    resolve: (v: T) => void;
    reject: (e: Error) => void;
};
declare module "ioredis" {
    interface Redis {
        blocked?: boolean;
    }
    interface Cluster {
        blocked?: boolean;
    }
}
/**
 * Redis Adapter for Workflows
 */
export default class RedisAdapter extends BaseAdapter {
    opts: RedisAdapterOptions;
    isWorker: boolean;
    commandClient: RedisClient | Cluster;
    blockerClient: RedisClient | Cluster;
    subClient: RedisClient | Cluster;
    signalPromises: Map<string, StoredPromise<unknown>>;
    jobResultPromises: Map<string, StoredPromise<unknown>>;
    running: boolean;
    disconnecting: boolean;
    prefix: string;
    serializer: Serializers.Base;
    wf: Workflow;
    broker: ServiceBroker;
    logger: Logger;
    mwOpts: WorkflowsMiddlewareOptions;
    /**
     * Constructor of adapter.
     */
    constructor(opts?: string | RedisAdapterOptions);
    /**
     * Initialize the adapter.
     *
     * @param wf
     * @param broker
     * @param logger
     * @param mwOpts - Middleware options.
     */
    init(wf: Workflow | null, broker: ServiceBroker, logger: Logger, mwOpts: WorkflowsMiddlewareOptions): void;
    /**
     * Connect to Redis.
     *
     * @returns Resolves when the connection is established.
     */
    connect(): Promise<void>;
    /**
     * Create a Redis client.
     *
     * @param name Connection name
     * @returns
     */
    createRedisClient(name: string): Promise<RedisClient>;
    createCommandClient(): Promise<void>;
    createBlockerClient(): Promise<void>;
    createSubscriptionClient(): Promise<void>;
    /**
     * Close the adapter connection
     *
     * @returns Resolves when the disconnection is complete.
     */
    disconnect(): Promise<void>;
    /**
     * Wait for Redis client to be ready. If it does not exist, it will create a new one.
     *
     * @param {string} workflowName - The name of the workflow.
     * @returns {Promise<Redis>} Resolves with the Redis client instance.
     *
    async isClientReady(workflowName) {
        return new Promise((resolve, reject) => {
            let client = this.jobClients.get(workflowName);
            if (client && client.status === "ready") {
                resolve(client);
            } else {
                if (!client) {
                    client = this.createRedisClient();
                    this.jobClients.set(workflowName, client);
                }

                const handleReady = () => {
                    client.removeListener("end", handleEnd);
                    client.removeListener("error", handleError);
                    resolve(client);
                };

                let lastError;
                const handleError = err => {
                    lastError = err;
                };

                const handleEnd = () => {
                    client.removeListener("ready", handleReady);
                    client.removeListener("error", handleError);
                    reject(lastError);
                };

                client.once("ready", handleReady);
                client.on("error", handleError);
                client.once("end", handleEnd);
            }
        });
    }*/
    /**
     * Close a Redis client.
     *
     * @param client
     */
    closeClient(client: RedisClient | Cluster): Promise<void>;
    /**
     * Start the job processor for the given workflow.
     */
    startJobProcessor(): void;
    /**
     * Stop the job processor for the given workflow.
     */
    stopJobProcessor(): void;
    /**
     * Job processor for the given workflow. Waits for a job in the waiting queue, moves it to the active queue, and processes it.
     *
     * @returns Resolves when the job is processed.
     */
    runJobProcessor(): Promise<void>;
    /**
     * Get job details and process it.
     *
     * @param jobId - The ID of the job to process.
     * @returns Resolves when the job is processed.
     */
    processJob(jobId: string): Promise<void>;
    /**
     * Set a lock for the job.
     *
     * @param {string} jobId - The ID of the job to lock.
     * @returns {Promise<Function>} Resolves with a function to unlock the job.
     */
    lock(jobId: string): Promise<() => Promise<void>>;
    /**
     * Get a job from Redis.
     *
     * @param workflowName - Workflow name
     * @param jobId - The ID of the job.
     * @param fields - The fields to retrieve or true to retrieve all fields.
     * @returns Resolves with the job object or null if not found.
     */
    getJob(workflowName: string, jobId: string, fields?: string[] | true): Promise<Job | null>;
    /**
     * Get job events from Redis.
     *
     * @param workflowName - The name of the workflow.
     * @param jobId - The ID of the job.
     * @returns Resolves with an array of job events.
     */
    getJobEvents(workflowName: string, jobId: string): Promise<JobEvent[]>;
    /**
     * Add a job event to Redis.
     *
     * @param workflowName - The name of the workflow.
     * @param jobId - The ID of the job.
     * @param event - The event object to add.
     * @returns Resolves when the event is added.
     */
    addJobEvent(workflowName: string, jobId: string, event: Partial<JobEvent>): Promise<void>;
    /**
     * Move a job to the completed queue.
     *
     * @param job - The job object.
     * @param result - The result of the job execution.
     * @returns Resolves when the job is moved to the completed queue.
     */
    moveToCompleted(job: Job, result: unknown): Promise<void>;
    /**
     * Calculate the backoff time for a job.
     *
     * @param retryAttempts
     * @returns The backoff time in milliseconds.
     */
    getBackoffTime(retryAttempts: number): number;
    /**
     * Move a job to the failed queue.
     *
     * @param job - The job object.
     * @param err - The error that caused the job to fail.
     * @returns Resolves when the job is moved to the failed queue.
     */
    moveToFailed(job: Job | string, err: Error | null): Promise<void>;
    /**
     * Save the state of a job.
     *
     * @param workflowName - The name of the workflow.
     * @param jobId - The ID of the job.
     * @param state - The state object to save.
     * @returns Resolves when the state is saved.
     */
    saveJobState(workflowName: string, jobId: string, state: unknown): Promise<void>;
    /**
     * Get state of a workflow run.
     *
     * @param workflowName
     * @param jobId
     * @returns Resolves with the state object or null if not found.
     */
    getState(workflowName: string, jobId: string): Promise<unknown>;
    /**
     * Trigger a named signal.
     *
     * @param signalName - The name of the signal.
     * @param key - The key associated with the signal.
     * @param payload - The payload of the signal.
     * @returns Resolves when the signal is triggered.
     */
    triggerSignal(signalName: string, key?: string, payload?: unknown): Promise<void>;
    /**
     * Remove a named signal.
     *
     * @param signalName - The name of the signal.
     * @param key - The key associated with the signal.
     * @returns Resolves when the signal is triggered.
     */
    removeSignal(signalName: string, key?: string): Promise<void>;
    /**
     * Wait for a named signal.
     *
     * @param signalName - The name of the signal.
     * @param key - The key associated with the signal.
     * @param opts Options for waiting for the signal.
     * @returns Resolves with the signal payload.
     */
    waitForSignal<TSignalResult = unknown>(signalName: string, key?: string, opts?: SignalWaitOptions): Promise<TSignalResult>;
    /**
     * Create a new job and push it to the waiting or delayed queue.
     *
     * @param {string} workflowName - The name of the workflow.
     * @param {Job} job - The job.
     * @param {Record<string, any>} opts - Additional options for the job.
     * @returns {Promise<Job>} Resolves with the created job object.
     */
    newJob(workflowName: string, job: Job, opts: Partial<CreateJobOptions>): Promise<Job>;
    /**
     * Send a delayed job promoteAt message to all workers.
     *
     * @param workflowName
     * @param jobId
     * @param promoteAt
     */
    sendDelayedJobPromoteAt(workflowName: string, jobId: string, promoteAt: number): Promise<void>;
    /**
     * Get the next delayed jobs maintenance time.
     *
     * @returns
     */
    getNextDelayedJobTime(): Promise<number | null>;
    /**
     * Serialize a job object for storage in Redis.
     *
     * @param job - The job object to serialize.
     * @returns The serialized job object.
     */
    serializeJob(job: Job): Job;
    /**
     * Deserialize a job object retrieved from Redis.
     *
     * @param {Job} job - The serialized job object.
     * @returns {Job} The deserialized job object.
     */
    deserializeJob(job: Job): Job;
    /**
     * Finish a parent job.
     *
     * @param workflowName
     * @param jobId
     */
    finishParentJob(workflowName: string, jobId: string): Promise<void>;
    /**
     * Reschedule a repeatable job based on its configuration.
     *
     * @param workflowName - The name of workflow.
     * @param job - The job object or job ID to reschedule.
     * @returns Resolves when the job is rescheduled.
     */
    newRepeatChildJob(workflowName: string, job: Job): Promise<void>;
    /**
     * Clean up the adapter store. Workflowname and jobId are optional.
     * If both are provided, the adapter should clean up only the job with the given ID.
     * If only the workflow name is provided, the adapter should clean up all jobs
     * related to that workflow.
     * If neither is provided, the adapter should clean up all jobs.
     *
     * @param workflowName
     * @param jobId
     * @returns
     */
    cleanUp(workflowName?: string, jobId?: string): Promise<void>;
    /**
     * Delete keys from Redis based on a pattern.
     *
     * @param pattern
     * @returns
     */
    deleteKeys(pattern: string): Promise<void>;
    /**
     * Delete keys on a Redis cluster.
     *
     * @param pattern
     * @returns
     */
    deleteKeysOnCluster(pattern: string): Promise<void>;
    /**
     * Delete keys on a Redis node.
     *
     * @param node
     * @param pattern
     * @returns
     */
    deleteKeysOnNode(node: RedisClient, pattern: string): Promise<void>;
    /**
     * Acquire a maintenance lock for a workflow.
     *
     * @param lockTime - The time to hold the lock in milliseconds.
     * @param lockName - The name of the lock.
     * @returns Resolves with true if the lock is acquired, false otherwise.
     */
    lockMaintenance(lockTime: number, lockName?: string): Promise<boolean>;
    /**
     * Process delayed jobs for a workflow and move them to the waiting queue if ready.
     *
     * @returns {Promise<void>} Resolves when delayed jobs are processed.
     */
    maintenanceDelayedJobs(): Promise<void>;
    /**
     * Check active jobs and if they timed out, move to failed jobs.
     *
     * @returns Resolves when delayed jobs are processed.
     */
    maintenanceActiveJobs(): Promise<void>;
    /**
     * Process stalled jobs for a workflow and move them back to the waiting queue.
     *
     * @returns Resolves when stalled jobs are processed.
     */
    maintenanceStalledJobs(): Promise<void>;
    /**
     * Move stalled job back to the waiting queue.
     *
     * @param jobId - The ID of the stalled job.
     * @returns Resolves when the job is moved to the failed queue.
     */
    moveStalledJobsToWaiting(jobId: string): Promise<void>;
    /**
     * Remove old jobs from a specified queue based on their age.
     *
     * @param queueName - The name of the queue (e.g., completed, failed).
     * @param retention - The age threshold in milliseconds for removing jobs.
     * @returns Resolves when old jobs are removed.
     */
    maintenanceRemoveOldJobs(queueName: string, retention: number): Promise<void>;
    /**
     * Release the maintenance lock for a workflow.
     *
     * @param lockName - The name of the lock to release.
     * @returns Resolves when the lock is released.
     */
    unlockMaintenance(lockName?: string): Promise<void>;
    /**
     * Get Redis key (for Workflow) for the given name and type.
     *
     * @param name - The name of the workflow or entity.
     * @param type - The type of the key (e.g., job, lock, events).
     * @param id - Optional ID to append to the key.
     * @returns The constructed Redis key.
     */
    getKey(name: string, type?: string, id?: string): string;
    /**
     * Get Redis key for a signal.
     *
     * @param signalName The name of the signal
     * @param key The key of the signal
     * @returns The constructed Redis key for the signal.
     */
    getSignalKey(signalName: string, key?: string): string;
    /**
     * Format the result of zrange command to an array of objects.
     *
     * @param list
     * @param timeField
     * @returns
     */
    formatZrangeResultToObject<TResult>(list: string[], timeField?: string): TResult[];
    /**
     * List all completed job IDs for a workflow.
     * @param workflowName
     * @returns
     */
    listCompletedJobs(workflowName: string): Promise<ListFinishedJobResult[]>;
    /**
     * List all failed job IDs for a workflow.
     * @param workflowName
     * @returns
     */
    listFailedJobs(workflowName: string): Promise<ListFinishedJobResult[]>;
    /**
     * List all delayed job IDs for a workflow.
     * @param workflowName
     * @returns
     */
    listDelayedJobs(workflowName: any): Promise<ListDelayedJobResult[]>;
    /**
     * List all active job IDs for a workflow.
     * @param workflowName
     * @returns
     */
    listActiveJobs(workflowName: any): Promise<ListJobResult[]>;
    /**
     * List all waiting job IDs for a workflow.
     * @param  workflowName
     * @returns
     */
    listWaitingJobs(workflowName: any): Promise<ListJobResult[]>;
    /**
     * Dump all Redis data for all workflows to JSON files.
     *
     * @param folder - The folder to save the dump files.
     * @param wfNames - The names of the workflows to dump.
     */
    dumpWorkflows(folder: string, wfNames: string[]): Promise<void>;
    /**
     * Dump all Redis data for a workflow to a JSON file.
     *
     * @param workflowName - The name of the workflow.
     * @param folder - The folder to save the dump files.
     * @returns Path to the dump file.
     */
    dumpWorkflow(workflowName: string, folder?: string): Promise<string>;
}
//# sourceMappingURL=redis.d.ts.map