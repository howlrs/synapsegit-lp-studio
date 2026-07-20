# Third-party notices

SynapseGit LP Studio depends on third-party software recorded by `Cargo.lock`
and `pnpm-lock.yaml`. Each component remains subject to its own license terms.
The license expression in package metadata is an inventory field, not a legal
interpretation or a grant to redistribute this application.

The local Docker build copies the following material into
`/usr/share/licenses/synapsegit-lp-studio`:

- this repository's unresolved evaluation notice;
- the exact SynapseGit v0.4.0 source license as
  `SynapseGit-v0.4.0-LICENSE`;
- Cargo metadata and pnpm production-license inventory for that build; and
- detected `LICENSE*`, `COPYING*`, `COPYRIGHT*`, and `NOTICE*` files from the
  resolved Rust and web dependency source trees.

This build-time corpus is intended to retain the notices delivered with the
resolved dependencies. It does not resolve the still-unrecorded LP Studio
license, brand rights, production permission, or redistribution permission.
Do not publish a prebuilt image or other binary until an authorized legal
review confirms that the exact corpus is complete and all necessary permissions
have been recorded.
