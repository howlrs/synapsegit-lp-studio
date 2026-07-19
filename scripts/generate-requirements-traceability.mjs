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
  "UX-002",
  "UX-003",
  "UX-010",
  "UX-011",
  "UX-012",
  "UX-015",
  "UX-REV-001",
  "UX-REV-002",
  "UX-REV-003",
  "DM-002",
  "API-001",
  "API-002",
  "API-005",
  "FR-PROJ-001",
  "FR-PROJ-002",
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
