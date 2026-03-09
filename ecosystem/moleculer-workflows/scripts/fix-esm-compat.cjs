// Post-build: patch the ESM moleculer-compat.js to use createRequire
// instead of the CJS-only `require` and `module` globals.
"use strict";
const fs = require("fs");
const path = require("path");

const file = path.join(__dirname, "../dist/esm/moleculer-compat.js");
let src = fs.readFileSync(file, "utf8");

// Replace the problematic Function-based require block with createRequire
const OLD = `const _rsCompat = new Function('require', 'return require("moleculer-rs-client/src/compat")')(\n// In CJS the module-scoped require is available; in ESM we fall back to\n// Node's global module loader through the Function constructor.\n// eslint-disable-next-line no-undef, @typescript-eslint/no-explicit-any\ntypeof require !== \"undefined\" ? require : module.createRequire(__filename));`;

const NEW = `import { createRequire as _createRequire } from "module";
const _rsCompat = _createRequire(import.meta.url)("moleculer-rs-client/src/compat");`;

if (src.includes("_rsCompat")) {
  src = src.replace(OLD, NEW);
  fs.writeFileSync(file, src);
  console.log("✅ Patched dist/esm/moleculer-compat.js for ESM createRequire");
} else {
  console.log("⚠️  Nothing to patch in dist/esm/moleculer-compat.js");
}
