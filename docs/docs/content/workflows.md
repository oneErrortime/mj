# Workflows

Reliable, durable workflow execution for Moleculer services — like Temporal.io or Restate, but built natively into Moleculer.

**JS Source:** `ecosystem/moleculer-workflows/`  
**Rust port:** 🔲 Planned  
**NPM:** `@moleculer/workflows`

## Overview

`@moleculer/workflows` adds durable, resumable workflow execution to Moleculer. Workflows survive broker restarts, support retries, concurrency control, signals, and history retention — backed by Redis (or a fake in-memory adapter for testing).

> The Rust port is planned. This page documents the JS module bundled in `ecosystem/moleculer-workflows/`.

## Features

- Durable workflow execution — survives restarts
- Retry policies with configurable backoff
- Concurrency control per workflow type
- Signal support (external events that can advance a workflow)
- Workflow history and retention
- Timeout support
- Delayed execution
- Batch workflow processing
- Serialization-agnostic (JSON default)
- Full TypeScript types

## Install

```bash
npm i @moleculer/workflows
```

## Quickstart

```js
const { ServiceBroker } = require("moleculer");
const WorkflowsMiddleware = require("@moleculer/workflows").Middleware;

const broker = new ServiceBroker({
    logger: true,
    middlewares: [WorkflowsMiddleware({ adapter: "Redis" })]
});

// Define a service with workflows
broker.createService({
    name: "users",
    workflows: {
        signupWorkflow: {
            timeout:     "1 day",
            retention:   "3 days",
            concurrency: 3,
            params: {
                email: { type: "email" },
                name:  { type: "string" }
            },

            async handler(ctx) {
                // 1. Create user record
                const user = await ctx.call("users.create", ctx.params);

                // 2. Send welcome email (retried automatically on failure)
                await ctx.call("mail.send", {
                    to: ctx.params.email,
                    template: "welcome"
                });

                // 3. Wait for email verification signal (up to 7 days)
                await ctx.waitForSignal("emailVerified", "7 days");

                // 4. Activate account
                await ctx.call("users.activate", { id: user.id });

                return { userId: user.id, activated: true };
            }
        }
    }
});

broker.start().then(async () => {
    // Run the workflow
    const result = await broker.wf.run("users.signupWorkflow", {
        email: "alice@example.com",
        name: "Alice"
    });
    console.log(result); // { userId: "...", activated: true }
});
```

## Workflow options

| Option | Default | Description |
|--------|---------|-------------|
| `timeout` | none | Max execution time (e.g. `"1 hour"`, `"2 days"`) |
| `retention` | `"7 days"` | How long to keep finished workflow history |
| `concurrency` | unlimited | Max concurrent executions of this workflow type |
| `params` | none | Parameter validation schema (uses fastest-validator) |
| `retries` | 3 | Max retries on handler error |
| `retryDelay` | 1000 | Initial retry delay in ms |

## Running workflows

```js
// Start a workflow (fire and forget)
await broker.wf.run("service.workflowName", params);

// Start and wait for result
const result = await broker.wf.run("service.workflowName", params, {
    waitForResult: true,
    timeout: 30_000
});

// Check workflow status
const status = await broker.wf.getStatus(workflowId);

// Send a signal to a running workflow
await broker.wf.signal(workflowId, "emailVerified", { verifiedAt: Date.now() });
```

## Adapters

| Adapter | Status | Description |
|---------|--------|-------------|
| `Redis` | ✅ Production | Persistent, cluster-safe via Redis |
| `Fake` | ✅ Testing | In-memory, no external dependencies |

### Redis adapter

```js
WorkflowsMiddleware({
    adapter: {
        type: "Redis",
        options: {
            redis: { host: "localhost", port: 6379 },
            prefix: "wf:"
        }
    }
})
```

## TypeScript support

Full TypeScript types are included. Use the typed `WorkflowContext`:

```typescript
import { WorkflowContext } from "@moleculer/workflows";

const signupWorkflow = {
    async handler(ctx: WorkflowContext<{ email: string; name: string }>) {
        const { email, name } = ctx.params;
        // fully typed params
    }
};
```

## Source

Complete implementation at `ecosystem/moleculer-workflows/src/`. TypeScript source with extensive integration tests at `ecosystem/moleculer-workflows/test/`.

Key files:

| File | Description |
|------|-------------|
| `src/middleware.ts` | Moleculer middleware entry point |
| `src/workflow.ts` | Workflow execution engine |
| `src/adapters/redis.ts` | Redis persistence adapter |
| `src/types.ts` | All TypeScript type definitions |
