import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const packageJson = JSON.parse(
  readFileSync(resolve(root, "package.json"), "utf8"),
);
const contractLock = JSON.parse(
  readFileSync(resolve(root, "docs/synapsegit-contract.lock.json"), "utf8"),
);
const cargoToml = readFileSync(resolve(root, "Cargo.toml"), "utf8");
const cargoLock = readFileSync(resolve(root, "Cargo.lock"), "utf8");
const pnpmLock = readFileSync(resolve(root, "pnpm-lock.yaml"), "utf8");
const toolchain = readFileSync(resolve(root, "rust-toolchain.toml"), "utf8");

function assert(condition, message) {
  if (!condition) throw new Error("runtime lock check: " + message);
}

assert(packageJson.private === true, "root package must remain private");
assert(
  packageJson.packageManager === "pnpm@10.33.0",
  "pnpm packageManager must be exact",
);
assert(packageJson.engines?.node === "24.14.1", "Node engine must be exact");
assert(packageJson.engines?.pnpm === "10.33.0", "pnpm engine must be exact");
assert(
  toolchain.includes('channel = "1.95.0"'),
  "Rust toolchain must be exact",
);

const revision = contractLock.revision;
assert(
  cargoToml.includes(`rev = "${revision}"`),
  "Cargo SynapseGit dependency must match the contract lock revision",
);
assert(
  cargoLock.includes(
    `git+https://github.com/howlrs/synapsegit?rev=${revision}#${revision}`,
  ),
  "Cargo.lock must resolve the exact SynapseGit revision",
);
assert(
  !cargoToml.includes("../synapsegit"),
  "committed Cargo dependencies must not depend on an adjacent checkout",
);

for (const pinned of [
  "react@19.2.7:",
  "react-dom@19.2.7:",
  "vite@8.1.5:",
  "typescript@7.0.2:",
  "'@playwright/test@1.61.1':",
]) {
  assert(pnpmLock.includes(pinned), `pnpm lock is missing ${pinned}`);
}

console.log(
  `runtime_locks_ok: node=${packageJson.engines.node} rust=1.95.0 synapsegit=${revision}`,
);
