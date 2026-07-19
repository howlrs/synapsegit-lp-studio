import { createHash } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { spawnSync } from "node:child_process";

const root = resolve(import.meta.dirname, "..");
const lockPath = resolve(root, "docs/synapsegit-contract.lock.json");
const lock = JSON.parse(readFileSync(lockPath, "utf8"));
const requireCheckout = process.argv.includes("--require-checkout");
const checkout = resolve(
  process.env.SYNAPSEGIT_CHECKOUT ?? resolve(root, "../synapsegit"),
);

function fail(message) {
  console.error("synapsegit contract lock: " + message);
  process.exit(1);
}

if (lock.lock_version !== 1) fail("lock_version must equal 1");
if (!/^[0-9a-f]{40}$/.test(lock.revision ?? "")) {
  fail("revision must be a full lowercase Git commit SHA");
}
if (lock.contract?.name !== "synapsegit.generic-artifact") {
  fail("unexpected contract name");
}
if (lock.contract?.version !== 1) fail("unexpected contract version");
if (
  JSON.stringify(lock.contract?.source_attributions) !==
  JSON.stringify(["caller_supplied_ai_attributed"])
) {
  fail("v1 must expose only caller-supplied attribution");
}
if (lock.contract?.execution_verified !== false) {
  fail("v1 execution must remain unverified");
}
if (!Array.isArray(lock.artifacts) || lock.artifacts.length !== 6) {
  fail("exactly six frozen upstream artifacts are required");
}
const paths = new Set();
for (const artifact of lock.artifacts) {
  if (
    typeof artifact.path !== "string" ||
    !artifact.path.startsWith(lock.contract.root + "/") ||
    artifact.path.includes("..")
  ) {
    fail("artifact path is outside the frozen contract root");
  }
  if (!/^[0-9a-f]{64}$/.test(artifact.sha256 ?? "")) {
    fail("artifact hash must be lowercase SHA-256: " + artifact.path);
  }
  if (paths.has(artifact.path)) fail("duplicate artifact path: " + artifact.path);
  paths.add(artifact.path);
}
for (const [key, expected] of Object.entries({
  host_authenticated_human_approval_required: true,
  workflow_journal_integration: false,
  restart_resumable_orchestrator: false,
  cryptographic_durable_admission_evidence: false,
  tagged_release_or_distribution_permission: false,
})) {
  if (lock.claim_boundaries?.[key] !== expected) {
    fail("claim boundary changed without review: " + key);
  }
}

if (!existsSync(resolve(checkout, ".git"))) {
  if (requireCheckout) fail("adjacent checkout is required at " + checkout);
  console.log("synapsegit_contract_lock_ok: checkout=absent revision=" + lock.revision);
  process.exit(0);
}

const commit = spawnSync(
  "git",
  ["-C", checkout, "cat-file", "-e", lock.revision + "^{commit}"],
  { encoding: "utf8" },
);
if (commit.status !== 0) fail("pinned revision is missing from adjacent checkout");

let capabilities;
for (const artifact of lock.artifacts) {
  const shown = spawnSync(
    "git",
    ["-C", checkout, "show", lock.revision + ":" + artifact.path],
    { encoding: null, maxBuffer: 4 * 1024 * 1024 },
  );
  if (shown.status !== 0) fail("cannot read pinned artifact: " + artifact.path);
  const actual = createHash("sha256").update(shown.stdout).digest("hex");
  if (actual !== artifact.sha256) {
    fail("hash mismatch for " + artifact.path + ": " + actual);
  }
  if (artifact.path.endsWith("/capabilities.json")) {
    capabilities = JSON.parse(shown.stdout.toString("utf8"));
  }
}
if (
  capabilities?.contract !== lock.contract.name ||
  capabilities?.contract_version !== lock.contract.version ||
  JSON.stringify(capabilities?.source_attributions) !==
    JSON.stringify(lock.contract.source_attributions)
) {
  fail("capabilities do not match the lock assertions");
}

console.log(
  "synapsegit_contract_lock_ok: checkout=verified revision=" + lock.revision,
);
