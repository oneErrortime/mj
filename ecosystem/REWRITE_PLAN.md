# Ecosystem JS Rewrite Plan

## Problem

All three packages under `ecosystem/` depend on the **Node.js `moleculer` package** as a peer dependency:

```
moleculer-channels   → require("moleculer")
moleculer-database   → require("moleculer")
moleculer-workflows  → import { ... } from "moleculer"
```

This repository is a **Rust reimplementation** of Moleculer. There is no Node.js moleculer broker here — only the Rust `ServiceBroker` that exposes an HTTP API via the Laboratory Agent on `:3210`.

As a result, **none of the ecosystem packages can run**.

---

## Root cause breakdown

| Package | Type | Issue |
|---------|------|-------|
| `moleculer-channels` | JS (CommonJS) | Hard-wires `require("moleculer")` everywhere |
| `moleculer-database` | JS (CommonJS) | Same, plus adapter layer depends on Moleculer lifecycle hooks |
| `moleculer-workflows` | TypeScript (ESM) | Same + no `dist/` build, no `node_modules`, depends on `moleculer#next` dev branch |

---

## Rewrite strategy

### Step 1 — Create `moleculer-rs-client` (this plan)
A minimal Node.js HTTP client that speaks the Rust broker's HTTP API (`:3210`).  
This becomes the **shared foundation** all ecosystem packages import instead of `moleculer`.

HTTP API surface (from `src/laboratory/`):
- `GET  /health`                  → broker health
- `GET  /info`                    → broker info + flags
- `GET  /services`                → registered services
- `GET  /actions`                 → all actions
- `GET  /topology`                → call-graph edges
- `GET  /metrics`                 → metrics snapshot
- `GET  /traces`                  → recent spans
- `GET  /logs`                    → ring-buffer logs
- `GET  /channels`                → channels + DLQ
- `GET  /cache`                   → LRU cache entries
- `GET  /circuit-breakers`        → CB states
- `POST /action`                  → call an action  `{ action, params }`
- `POST /channels/send`           → publish to channel `{ channel, payload }`
- `DELETE /channels/dlq/:id`      → retry / discard DLQ entry

### Step 2 — Rewrite `moleculer-channels`
Replace all `require("moleculer")` calls with `moleculer-rs-client`.  
Keep the public API (subscribe / publish / ack / nack) intact but route through HTTP.

### Step 3 — Rewrite `moleculer-workflows`
Rewrite TypeScript source; replace moleculer types with client types.  
Fix build (tsconfig, tsup), generate proper `dist/`.

### Step 4 — Rewrite `moleculer-database`
Most complex — depends on Moleculer service lifecycle.  
Will be adapted to use action-call pattern over HTTP.

---

## Progress

- [x] Step 0 — Analysis & plan (this document)
- [ ] Step 1 — `moleculer-rs-client` package
- [ ] Step 2 — `moleculer-channels` rewrite
- [ ] Step 3 — `moleculer-workflows` rewrite
- [ ] Step 4 — `moleculer-database` rewrite
