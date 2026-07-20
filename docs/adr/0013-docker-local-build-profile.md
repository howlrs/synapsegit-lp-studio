# ADR-0013: Docker local-build evaluation profile

Status: accepted for local non-commercial evaluation; public image distribution prohibited

Date: 2026-07-20

## Context

The verified application baseline is Linux x86-64 GNU, and the primary Creator
uses Windows with WSL2. Installing the pinned Node, pnpm, and Rust toolchains in
that WSL distribution expands the persistent host environment. A container can
isolate build/runtime dependencies and retain only application state in a
Docker-managed volume.

The native server deliberately binds Editor and Preview only to IPv4 loopback.
Docker bridge publication cannot reach a listener bound to the container's own
loopback. Docker Desktop host networking is an optional feature and is not a
portable prerequisite. Preview URLs also embed the external port in scoped
`*.localhost` origins, Host validation, and CSP, so host/container port
translation is not valid.

SynapseGit v0.4.0's Source-Available License section 4(d) does not grant
permission to publish a container image, GitHub Package, GitHub Release, or
downloadable binary distribution. LP Studio itself still has no recorded
general license grant.

## Decision

- Provide a source Dockerfile and Compose profile that each evaluator builds
  locally. Do not publish a prebuilt image or configure registry credentials.
- Support only Linux `amd64` containers and Chromium on the host. Windows/WSL2
  is a usage target whose manual browser evidence remains pending.
- Use an exact `LP_STUDIO_CONTAINER_MODE=1` opt-in. It requires distinct,
  explicit, unprivileged Editor and Preview ports and binds the two listeners
  to the container interface.
- Keep browser-visible authority loopback-only: Editor is
  `http://127.0.0.1:4173`; Preview uses scoped `*.localhost:4174` origins.
  The application continues exact Host, Origin, Fetch Metadata, session, and
  scoped Preview-route validation. It does not trust forwarded headers.
- The supported Compose profile publishes `4173` and `4174` only on host
  `127.0.0.1`, with identical host/container ports. Arbitrary `docker run -p`,
  reverse proxies, LAN publication, alternate hostnames, and port remapping are
  unsupported.
- Require Docker Engine 28 or newer. Earlier engines had a known localhost
  publication weakness that could permit same-L2 access.
- Run as fixed UID/GID 65532 with a read-only root filesystem, no Linux
  capabilities, `no-new-privileges`, bounded `/tmp`, and graceful SIGTERM.
- Use the application binary's exact `--healthcheck` mode to verify both the
  Editor and Preview role responses without adding a shell HTTP client to the
  runtime image. The healthcheck does not open managed state or provider data.
- Persist state in a Docker named volume initialized from an owner-only `0700`
  directory. Do not recommend Windows filesystem state bind mounts. Import is
  a separate existing directory mounted read-only with automatic host-path
  creation disabled.
- Pass an optional provider credential from standard input into a dedicated
  Docker named volume and mount that volume read-only in the application
  container; never bake it into an image, Compose file, host environment, or
  container environment metadata. Retain the native process environment option
  for non-container use.
- Include a CA certificate bundle, the unresolved LP Studio evaluation notice,
  the hash-verified SynapseGit v0.4.0 license, dependency inventories, and the
  license/notice files detected in resolved build sources. This is notice
  retention for a local build, not legal clearance for redistribution.

## Distribution boundary

The Dockerfile and Compose files are build instructions in the development
repository. They are not a released image or a grant to redistribute either LP
Studio or SynapseGit. GHCR/Docker Hub push, image attestation, version tag, and
release automation remain prohibited until the exact artifact and destination
have written permission from all applicable rights holders and LP Studio has
recorded distribution terms.

## Verification

Repository checks reject unpinned base images, non-loopback Compose
publication, host networking, port translation, a privileged runtime, writable
root filesystem, and any workflow path that pushes a container image. Linux CI
builds the image, inspects the runtime controls, exercises both health roles,
checks exact Host rejection, runs a Chromium package-browser smoke through the
published Editor port, verifies volume persistence across recreation, and
stops/removes the test deployment.

Windows Chromium must still verify scoped `*.localhost` Preview behavior and
LAN non-reachability before Windows/WSL2 Docker support can be called complete.
