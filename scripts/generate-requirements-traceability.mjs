import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const sourcePath = resolve(root, "docs/detailed-requirements.md");
const outputPath = resolve(root, "docs/requirements-traceability.md");
const lines = readFileSync(sourcePath, "utf8").split("\n");

const implementedRequirements = new Set([
  "INT-SG-001",
  "INT-SG-002",
  "INT-SG-004",
  "INT-SG-MAP-001",
  "INT-SG-CLAIM-001",
  "INT-SG-CLAIM-002",
  "INT-SG-CLAIM-003",
  "INT-SG-AVAIL-002",
  "ARCH-001",
  "ARCH-002",
  "ARCH-003",
  "ARCH-004",
  "ARCH-005",
  "ARCH-006",
  "UX-001",
  "UX-003",
  "UX-005",
  "UX-006",
  "UX-010",
  "UX-011",
  "UX-012",
  "UX-015",
  "UX-REV-001",
  "UX-REV-002",
  "UX-REV-003",
  "API-001",
  "API-002",
  "API-004",
  "API-005",
  "FR-PROJ-001",
  "FR-PROJ-002",
  "FR-PROJ-003",
  "FR-PROJ-004",
  "FR-PROJ-005",
  "FR-PROJ-006",
  "FR-FILE-001",
  "FR-FILE-002",
  "FR-FILE-003",
  "FR-FILE-006",
  "FR-FILE-007",
  "DM-REV-001",
  "DM-REV-002",
  "DM-REV-003",
  "DM-REV-004",
  "DM-REV-005",
  "FR-PREV-001",
  "FR-PREV-002",
  "FR-PREV-003",
  "FR-PREV-004",
  "FR-PREV-005",
  "SEC-PREV-001",
  "SEC-PREV-002",
  "SEC-PREV-003",
  "SEC-PREV-004",
  "SEC-PREV-005",
  "SEC-PREV-006",
  "SEC-PREV-007",
  "SEC-PREV-008",
  "FR-PREV-010",
  "FR-PREV-011",
  "FR-PREV-012",
  "FR-PREV-013",
  "FR-TGT-001",
  "FR-TGT-002",
  "DM-TGT-001",
  "DM-TGT-002",
  "DM-TGT-003",
  "DM-TGT-004",
  "DM-TGT-005",
  "DM-TGT-006",
  "DM-TGT-007",
  "FR-COORD-001",
  "FR-COORD-002",
  "FR-COORD-003",
  "FR-COORD-004",
  "FR-COORD-005",
  "FR-COORD-006",
  "FR-COORD-007",
  "UX-COORD-001",
  "FR-BLOCK-001",
  "FR-BLOCK-002",
  "FR-BLOCK-003",
  "FR-BLOCK-004",
  "FR-RESOLVE-001",
  "FR-RESOLVE-003",
  "FR-RESOLVE-006",
  "FR-CONV-003",
  "FR-CONV-005",
  "FR-CTX-002",
  "FR-CTX-003",
  "FR-CTX-004",
  "FR-CTX-005",
  "FR-CTX-008",
  "FR-CTX-009",
  "INT-AI-002",
  "INT-AI-003",
  "INT-AI-004",
  "INT-AI-005",
  "INT-AI-006",
  "INT-AI-007",
  "FR-CHG-001",
  "FR-CHG-002",
  "FR-CHG-003",
  "FR-CHG-004",
  "FR-CHG-005",
  "FR-CHG-006",
  "FR-CHG-007",
  "FR-CHG-008",
  "FR-CHG-009",
  "FR-CHG-010",
  "FR-CHG-011",

  // C7: durable Proposal review, whole-Proposal Human Decision, and recovery.
  // Interrupted generation remains planned because only the Decision saga has
  // complete durable-boundary/process-abort evidence.
  "DM-PROP-001",
  "DM-PROP-002",
  "DM-PROP-006",
  "UX-REV-004",
  "UX-REV-005",
  "UX-REV-006",
  "UX-REV-007",
  "FR-DEC-001",
  "FR-DEC-002",
  "FR-DEC-003",
  "FR-DEC-004",
  "FR-DEC-005",
  "FR-DEC-006",
  "FR-DEC-007",
  "FR-DEC-008",
  "FR-DEC-009",
  "FR-REC-001",
  "FR-REC-002",
  "FR-REC-003",
  "FR-REC-004",
  "FR-REC-005",
  "FR-REC-006",
  "DEP-SG-001",
  "INT-SG-MAP-002",
  "INT-SG-MAP-003",
  "INT-SG-MAP-004",
  "INT-SG-MAP-005",
  "INT-SG-MAP-006",
  "INT-SG-CLAIM-004",
  "INT-SG-AVAIL-004",

  // C8: the supported destination is a deterministic downloadable archive;
  // no arbitrary directory-destination API is claimed by these entries.
  "FR-EXP-001",
  "FR-EXP-002",
  "FR-EXP-003",
  "FR-EXP-004",
  "FR-EXP-005",
  "FR-EXP-010",
  "FR-EXP-011",
  "FR-EXP-012",
  "FR-EXP-013",
  "FR-EXP-014",
  "FR-EXP-015",
  "FR-EXP-016",
  "NFR-EXP-001",
  "NFR-EXP-002",
  "DM-EXP-001",
  "FR-EXP-018",
  "INT-PUB-001",
  "INT-PUB-002",
  "INT-PUB-003",
  "DM-PUB-001",
  "DM-PUB-002",
  "DM-PUB-003",
  "DM-PUB-004",
  "DM-PUB-005",
  "UX-PUB-001",
  "UX-PUB-002",
  "INT-PUB-011",

  // C9: loopback/content/filesystem hardening, migration/recovery, and exact
  // manual retention. Telemetry and automatic GC are deliberately absent.
  "DM-005",
  "SEC-APP-001",
  "SEC-APP-002",
  "SEC-APP-003",
  "SEC-APP-004",
  "SEC-APP-005",
  "SEC-APP-006",
  "SEC-APP-007",
  "SEC-APP-008",
  "SEC-CONT-001",
  "SEC-CONT-002",
  "SEC-CONT-003",
  "SEC-CONT-004",
  "SEC-FS-001",
  "SEC-FS-002",
  "SEC-FS-003",
  "SEC-FS-004",
  "SEC-FS-005",
  "SEC-PRIV-002",
  "SEC-PRIV-001",
  "SEC-PRIV-003",
  "SEC-PRIV-004",
  "SEC-PRIV-005",
  "SEC-PRIV-006",
  "NFR-REL-002",
  "NFR-REL-001",
  "NFR-REL-003",
  "NFR-REL-004",
  "NFR-REL-005",
  "NFR-REL-006",
  "NFR-REL-007",
  "NFR-REL-008",
  "NFR-REL-010",

  // C10/C11: only hard automated geometry/runtime invariants, automated
  // accessibility behavior, observability, test strategy, and acceptance
  // scenarios exercised by the checked-in suites are promoted here.
  "NFR-PERF-002",
  "NFR-PERF-003",
  "NFR-PERF-004",
  "NFR-A11Y-002",
  "NFR-A11Y-003",
  "NFR-A11Y-004",
  "NFR-A11Y-005",
  "NFR-A11Y-006",
  "NFR-A11Y-008",
  "NFR-I18N-001",
  "NFR-I18N-003",
  "NFR-COMP-002",
  "NFR-OBS-001",
  "NFR-OBS-002",
  "NFR-OBS-003",
  "NFR-OBS-004",
  "NFR-OBS-005",
  "NFR-OBS-006",
  "TEST-001",
  "TEST-002",
  "TEST-003",
  "TEST-004",
  "TEST-005",
  "TEST-006",
  "AC-001",
  "AC-002",
  "AC-006",
  "AC-008",
  "AC-009",
  "AC-010",
  "AC-011",
  "AC-012",
  "AC-014",
  "AC-016",
  "AC-017",
  "AC-019",
  "AC-020",
  "AC-021",
  "AC-023",
  "AC-024",
  "AC-025",
  "AC-026",
]);

// These gates intentionally stay planned until their distinct manual,
// external, or application-performance evidence exists. Keeping the list in
// the generator makes an accidental broad checkpoint promotion fail closed.
const mustRemainPlannedRequirements = new Set([
  "DM-PROP-003",
  "DM-PROP-004",
  "DM-PROP-005",
  "INT-SG-003",
  "INT-SG-005",
  "INT-SG-AVAIL-001",
  "INT-SG-AVAIL-003",
  "INT-SG-AVAIL-005",
  "DEP-LIC-001",
  "DEP-LIC-002",
  "DEP-PLAT-001",
  "FR-EXP-006",
  "FR-EXP-007",
  "FR-EXP-017",
  "DM-PUB-006",
  "INT-PUB-010",
  "INT-PUB-012",
  "SEC-PRIV-007",
  "NFR-REL-009",
  "NFR-PERF-001",
  "NFR-PERF-005",
  "NFR-PERF-006",
  "NFR-PERF-007",
  "NFR-PERF-008",
  "NFR-A11Y-001",
  "NFR-A11Y-007",
  "NFR-A11Y-009",
  "NFR-COMP-001",
  "NFR-COMP-003",
  "NFR-COMP-004",
  "NFR-OBS-007",
  "AC-003",
  "AC-004",
  "AC-005",
  "AC-007",
  "AC-013",
  "AC-015",
  "AC-018",
  "AC-022",
  "AC-027",
  "AC-028",
  "DEV-003",
  "DEV-005",
  "DEV-007",
]);

function statusFor(id) {
  if (!implementedRequirements.has(id)) return "planned";
  const checkpoint = checkpointFor(id);
  const evidenceAnchor = checkpoint.toLowerCase();
  return (
    "implemented — [" +
    checkpoint +
    " evidence](implementation-status.md#" +
    evidenceAnchor +
    "-evidence-and-limits)"
  );
}

function checkpointFor(id) {
  if (id.startsWith("DEV-")) return "C0–C11";
  if (["UX-005", "UX-006", "API-004"].includes(id)) return "C10";
  if (id.startsWith("DEP-LIC") || id.startsWith("DEP-PLAT")) return "C0 / M2";
  if (id.startsWith("DEP-SG") || id.startsWith("INT-SG")) return "C1";
  if (
    id.startsWith("FR-PROJ") ||
    id.startsWith("FR-FILE") ||
    id.startsWith("DM-REV")
  )
    return "C3";
  if (id.includes("PREV")) return "C4";
  if (
    id.includes("TGT") ||
    id.includes("COORD") ||
    id.includes("BLOCK") ||
    id.includes("RESOLVE")
  )
    return "C5";
  if (
    id.includes("AI-") ||
    id.includes("CTX") ||
    id.includes("CONV") ||
    id.includes("CHG")
  )
    return "C6";
  if (id.includes("PROP") || id.includes("DEC") || id.startsWith("FR-REC"))
    return "C7";
  if (id.includes("EXP") || id.includes("PUB")) return "C8";
  if (id.startsWith("SEC-") || id.includes("REL")) return "C9";
  if (
    id.startsWith("AC-") ||
    id.startsWith("TEST-") ||
    id.includes("PERF") ||
    id.includes("A11Y") ||
    id.includes("COMP") ||
    id.includes("I18N") ||
    id.includes("OBS")
  )
    return "C10";
  if (id.startsWith("RECOMMEND-")) return "M2";
  return "C2";
}

function evidenceFor(id) {
  if (id.startsWith("DEV-")) return "Git/Synapse checkpoint + PR evidence";
  if (id.startsWith("AC-")) return "Acceptance scenario";
  if (id.startsWith("TEST-")) return "Test-suite/meta validation";
  if (id.startsWith("SEC-")) return "Security unit/integration/browser test";
  if (id.includes("PERF")) return "Pinned performance fixture";
  if (id.includes("A11Y")) return "Automated + manual accessibility evidence";
  if (id.startsWith("DEP-")) return "ADR + capability/license evidence";
  if (id.startsWith("RECOMMEND-")) return "Backlog decision";
  if (id.startsWith("UX-")) return "Browser E2E + manual UX review";
  if (id.startsWith("DM-") || id.startsWith("API-"))
    return "Schema/contract test";
  return "Unit/integration/E2E as applicable";
}

const requirements = [];
let section = "";

for (let index = 0; index < lines.length; index += 1) {
  const line = lines[index];
  const sectionMatch = line.match(/^##\s+(.+)$/);
  if (sectionMatch) section = sectionMatch[1];

  const requirementMatch = line.match(
    /^- \*\*([A-Z][A-Z0-9-]+)(?:\s+\/\s+([^*]+))?:\*\*\s*(.*)$/,
  );
  if (!requirementMatch) continue;

  const [, id, priority = "", firstSummary] = requirementMatch;
  const summaryParts = [firstSummary];
  let continuation = index + 1;
  while (
    continuation < lines.length &&
    /^\s{2,}\S/.test(lines[continuation]) &&
    !/^\s{2,}[-*]\s/.test(lines[continuation])
  ) {
    summaryParts.push(lines[continuation].trim());
    continuation += 1;
  }

  const summary = summaryParts
    .join(" ")
    .replace(/\|/g, "\\|")
    .replace(/\x60/g, "")
    .trim();
  requirements.push({
    id,
    priority: priority.trim() || "—",
    section,
    line: index + 1,
    checkpoint: checkpointFor(id),
    evidence: evidenceFor(id),
    summary,
  });
}

const duplicates = requirements
  .map(({ id }) => id)
  .filter((id, index, ids) => ids.indexOf(id) !== index);
if (duplicates.length > 0) {
  throw new Error(
    "Duplicate requirement IDs: " + [...new Set(duplicates)].join(", "),
  );
}
if (requirements.length === 0) {
  throw new Error("No requirements found");
}

const requirementIds = new Set(requirements.map(({ id }) => id));
const unknownClassifiedRequirements = [
  ...implementedRequirements,
  ...mustRemainPlannedRequirements,
].filter((id) => !requirementIds.has(id));
if (unknownClassifiedRequirements.length > 0) {
  throw new Error(
    "Unknown classified requirement IDs: " +
      [...new Set(unknownClassifiedRequirements)].sort().join(", "),
  );
}
const incorrectlyPromotedRequirements = [...mustRemainPlannedRequirements]
  .filter((id) => implementedRequirements.has(id))
  .sort();
if (incorrectlyPromotedRequirements.length > 0) {
  throw new Error(
    "Requirements with pending gates cannot be implemented: " +
      incorrectlyPromotedRequirements.join(", "),
  );
}

const output = [
  "# Requirement traceability",
  "",
  "Status: generated active implementation matrix",
  "",
  "Source: [detailed-requirements.md](detailed-requirements.md)",
  "",
  "Generated by scripts/generate-requirements-traceability.mjs.",
  "Do not edit the matrix manually. Checkpoint status/evidence links are updated",
  "by extending the generator input once implementation begins.",
  "An implemented row records requirement-level code and automated evidence; it does not",
  "promote a checkpoint or satisfy a distinct manual/external gate. Creator review, manual",
  "keyboard/zoom/contrast/screen-reader evidence, live-provider execution, license/brand,",
  "merge/release/distribution remain pending. Conversation streaming and cancellation for",
  "non-AI long operations also remain outside the implemented baseline.",
  "",
  "Requirement count: " + requirements.length,
  "",
  "| Requirement | Priority | Checkpoint | Planned evidence | Source section | Summary | Status |",
  "| --- | --- | --- | --- | --- | --- | --- |",
  ...requirements.map(
    ({
      id,
      priority,
      checkpoint,
      evidence,
      section: sourceSection,
      line,
      summary,
    }) =>
      "| [" +
      id +
      "](detailed-requirements.md#L" +
      line +
      ") | " +
      priority +
      " | " +
      checkpoint +
      " | " +
      evidence +
      " | " +
      sourceSection.replace(/\|/g, "\\|") +
      " | " +
      summary +
      " | " +
      statusFor(id) +
      " |",
  ),
  "",
].join("\n");

if (process.argv.includes("--check")) {
  const current = readFileSync(outputPath, "utf8");
  if (current !== output) {
    throw new Error("requirements-traceability.md is stale; regenerate it");
  }
  console.log(
    "traceability is current for " + requirements.length + " requirements",
  );
} else {
  writeFileSync(outputPath, output, "utf8");
  console.log(
    "wrote " + requirements.length + " requirements to " + outputPath,
  );
}
