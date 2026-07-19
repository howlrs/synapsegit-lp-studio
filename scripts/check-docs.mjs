import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, extname, resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");

function filesBelow(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = resolve(directory, entry.name);
    return entry.isDirectory() ? filesBelow(path) : [path];
  });
}

const markdownFiles = [
  resolve(root, "README.md"),
  ...filesBelow(resolve(root, "docs")).filter((path) => extname(path) === ".md"),
];
const errors = [];
const requirementIds = [];

for (const path of markdownFiles) {
  const content = readFileSync(path, "utf8");
  const lines = content.split("\n");
  lines.forEach((line, index) => {
    if (/[ \t]+$/.test(line)) {
      errors.push(path + ":" + (index + 1) + " has trailing whitespace");
    }
  });

  const fenceCount = lines.filter((line) => /^\s*```/.test(line)).length;
  if (fenceCount % 2 !== 0) {
    errors.push(path + " has an unbalanced fenced code block");
  }

  for (const match of content.matchAll(/\[[^\]]+\]\(([^)]+)\)/g)) {
    const target = match[1];
    if (
      target.startsWith("http://") ||
      target.startsWith("https://") ||
      target.startsWith("#") ||
      target.startsWith("mailto:")
    ) continue;
    const fileTarget = target.split("#", 1)[0];
    if (fileTarget && !existsSync(resolve(dirname(path), fileTarget))) {
      errors.push(path + " links to missing " + target);
    }
  }

  for (const match of content.matchAll(/^- \*\*([A-Z][A-Z0-9-]+)(?:\s+\/|:)/gm)) {
    requirementIds.push(match[1]);
  }
}

const duplicateIds = requirementIds.filter(
  (id, index, ids) => ids.indexOf(id) !== index,
);
if (duplicateIds.length > 0) {
  errors.push("duplicate requirement IDs: " + [...new Set(duplicateIds)].join(", "));
}

const plan = readFileSync(resolve(root, "docs/implementation-plan.md"), "utf8");
const weights = [...plan.matchAll(/^\| C\d+ [^|]+ \| (\d+)% \|/gm)].map(
  (match) => Number(match[1]),
);
const totalWeight = weights.reduce((total, weight) => total + weight, 0);
if (totalWeight !== 100) {
  errors.push("implementation checkpoint weights total " + totalWeight + "%, not 100%");
}

const traceability = readFileSync(
  resolve(root, "docs/requirements-traceability.md"),
  "utf8",
);
const traceabilityRows = (
  traceability.match(/^\| \[[A-Z][A-Z0-9-]+\]/gm) ?? []
).length;
if (traceabilityRows !== requirementIds.length) {
  errors.push(
    "traceability rows " + traceabilityRows +
    " do not match requirement IDs " + requirementIds.length,
  );
}

const nonFiles = markdownFiles.filter((path) => !statSync(path).isFile());
if (nonFiles.length > 0) {
  errors.push("non-file documentation paths: " + nonFiles.join(", "));
}

if (errors.length > 0) {
  console.error(errors.join("\n"));
  process.exit(1);
}

console.log(
  "checked " + markdownFiles.length + " Markdown files, " +
  requirementIds.length + " unique requirements, and 100% checkpoint weight",
);
