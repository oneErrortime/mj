// compat-loader.cts
// CommonJS entry point to load the moleculer-rs-client compat shim.
// This file is compiled as .cjs by TypeScript so it can use require() safely
// in both CJS and ESM contexts.

// eslint-disable-next-line @typescript-eslint/no-require-imports
const { ServiceBroker: _ServiceBroker } = require("moleculer-rs-client/src/compat");

// Re-export as typed constructor
export = _ServiceBroker as new (opts?: Record<string, unknown>) => unknown;
