# ADR-0006: Source-available license and brand evaluation boundary

Status: accepted evaluation constraint

Date: 2026-07-19

## Context

The audited SynapseGit v0.3.0 license is source-available, not OSI-approved.
Its stated Production Use definition includes a live service, external
deliverable, operational process, and operational decision. It does not
generally grant production/commercial/hosted use, redistribution, or use of
the SynapseGit name/logo/trademarks without separate written permission.

## Decision

- C0–C11 work is a local/internal evaluation and development activity.
- Do not publish a binary distribution, hosted service, production claim,
  external deliverable, release, container, package, or SynapseGit brand claim
  under this plan.
- Record the responsible owner and obtain applicable Rights Holder permission
  before an M2 external/production/distribution action.
- Preserve SynapseGit and third-party notices in permitted source/evaluation
  copies.
- Keep LP Studio, generated LP content, template/font/image rights, and
  SynapseGit license obligations separate.

## Consequences

- The private development branch/draft PR is progress evidence, not a product
  distribution.
- C11 reports license/brand status; it does not silently grant permission.
- Any later release action requires a dedicated review of the exact artifact,
  destination, and terms.
