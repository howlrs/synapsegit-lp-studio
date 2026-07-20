import { readFileSync, readdirSync } from "node:fs";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const read = (path) => readFileSync(resolve(root, path), "utf8");

const dockerfile = read("Dockerfile");
const compose = read("compose.yaml");
const importCompose = read("compose.import.yaml");
const openAiCompose = read("compose.openai.yaml");
const openAiInitializer = read("scripts/init-docker-openai-secret.sh");
const main = read("apps/local-server/src/main.rs");
const licenseNotice = read("LICENSE");
const thirdPartyNotices = read("THIRD-PARTY-NOTICES.md");
const packageVerifier = read("scripts/verify-local-evaluation-package.mjs");

const assert = (condition, message) => {
  if (!condition) throw new Error(`docker distribution check: ${message}`);
};

assert(
  dockerfile.startsWith(
    "# syntax=docker/dockerfile:1.7@sha256:a57df69d0ea827fb7266491f2813635de6f17269be881f696fbfdf2d83dda33e",
  ),
  "Dockerfile frontend must be pinned by digest",
);

for (const image of [
  "node:24.14.1-bookworm-slim@sha256:",
  "rust:1.95.0-slim-bookworm@sha256:",
  "debian:bookworm-slim@sha256:",
]) {
  assert(
    dockerfile.includes(`FROM ${image}`),
    `base image is not pinned: ${image}`,
  );
}
assert(
  dockerfile.includes("USER 65532:65532"),
  "runtime image must use the fixed non-root UID",
);
assert(
  dockerfile.includes("-m 0700 /run/secrets"),
  "runtime image must seed the secret volume with private non-root ownership",
);
assert(
  dockerfile.includes("LP_STUDIO_CONTAINER_MODE=1") &&
    dockerfile.includes("LP_STUDIO_EDITOR_PORT=4173") &&
    dockerfile.includes("LP_STUDIO_PREVIEW_PORT=4174"),
  "runtime image must use the reviewed fixed container profile",
);
assert(
  dockerfile.includes(
    'org.opencontainers.image.licenses="LicenseRef-SynapseGit-LP-Studio-Evaluation"',
  ),
  "image must retain the unresolved evaluation license identifier",
);
for (const invariant of [
  "SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt",
  "/usr/share/licenses/synapsegit-lp-studio",
  "SynapseGit-v0.4.0-LICENSE",
  "200d6c727d7b3b62c85e7672c5615d21e5081fd95b8935acc5424bc98305415e",
  "cargo-metadata.json",
  "pnpm-production-licenses.json",
]) {
  assert(
    dockerfile.includes(invariant),
    `missing image invariant: ${invariant}`,
  );
}
assert(
  licenseNotice.includes("The identifier is metadata, not permission.") &&
    thirdPartyNotices.includes("Do not publish a prebuilt image"),
  "the unresolved license and third-party notice boundary is missing",
);
assert(
  packageVerifier.includes("SynapseGit-v0.4.0-LICENSE") &&
    packageVerifier.includes(
      "200d6c727d7b3b62c85e7672c5615d21e5081fd95b8935acc5424bc98305415e",
    ),
  "the local package verifier must retain the exact SynapseGit license too",
);

assert(
  (compose.match(/host_ip: 127\.0\.0\.1/g) ?? []).length === 2,
  "Editor and Preview must both publish only on host IPv4 loopback",
);
for (const port of ["4173", "4174"]) {
  assert(
    compose.includes(`target: ${port}`) &&
      compose.includes(`published: "${port}"`),
    `host and container port ${port} must remain identical`,
  );
}
for (const invariant of [
  'restart: "no"',
  'user: "65532:65532"',
  "read_only: true",
  "- ALL",
  "no-new-privileges:true",
  "source: studio-state",
  "--healthcheck",
]) {
  assert(
    compose.includes(invariant),
    `missing Compose invariant: ${invariant}`,
  );
}
assert(
  !compose.includes("network_mode: host"),
  "host networking is not supported",
);
assert(
  !compose.includes("ghcr.io"),
  "prebuilt registry images are not permitted",
);
assert(
  !compose.includes("OPENAI_API_KEY:"),
  "the base profile must not copy a provider key into container environment metadata",
);
assert(
  openAiCompose.includes("OPENAI_API_KEY_FILE: /run/secrets/openai-api-key") &&
    openAiCompose.includes("source: openai-secret") &&
    openAiCompose.includes("target: /run/secrets") &&
    openAiCompose.includes("read_only: true") &&
    openAiCompose.includes("external: true") &&
    openAiCompose.includes("synapsegit-lp-studio-openai-secret"),
  "the optional provider profile must mount an external secret volume read-only",
);
assert(
  openAiInitializer.includes("docker volume create") &&
    openAiInitializer.includes("--network none") &&
    openAiInitializer.includes("--user 65532:65532") &&
    openAiInitializer.includes("--cap-drop ALL") &&
    openAiInitializer.includes("no-new-privileges") &&
    openAiInitializer.includes('cat > "$secret_tmp"') &&
    !openAiInitializer.includes("OPENAI_API_KEY="),
  "the provider credential initializer must accept stdin without exposing the key in metadata",
);
assert(
  importCompose.includes("read_only: true") &&
    importCompose.includes("create_host_path: false"),
  "import override must use an existing read-only source",
);
assert(
  main.includes(
    'const CONTAINER_MODE_ENV: &str = "LP_STUDIO_CONTAINER_MODE";',
  ) &&
    main.includes(
      'const OPENAI_API_KEY_FILE_ENV: &str = "OPENAI_API_KEY_FILE";',
    ) &&
    main.includes("Ipv4Addr::UNSPECIFIED") &&
    main.includes("Ipv4Addr::LOCALHOST"),
  "the explicit container bind/public-origin separation is missing",
);

const workflowRoot = resolve(root, ".github/workflows");
for (const name of readdirSync(workflowRoot)) {
  if (!name.endsWith(".yml") && !name.endsWith(".yaml")) continue;
  const workflow = read(`.github/workflows/${name}`);
  for (const forbidden of [
    /packages:\s*write/u,
    /\bghcr\.io\b/u,
    /\bdocker\s+push\b/u,
    /\bpush:\s*true\b/u,
    /lp-studio-local-evaluation-\$\{\{/u,
  ]) {
    assert(
      !forbidden.test(workflow),
      `${name} contains a forbidden image publication path`,
    );
  }
}

console.log(
  "docker_distribution_ok: local-build-only linux/amd64 loopback profile",
);
