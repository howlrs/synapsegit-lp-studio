# syntax=docker/dockerfile:1.7@sha256:a57df69d0ea827fb7266491f2813635de6f17269be881f696fbfdf2d83dda33e

FROM node:24.14.1-bookworm-slim@sha256:b506e7321f176aae77317f99d67a24b272c1f09f1d10f1761f2773447d8da26c AS web-builder

WORKDIR /src

COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY apps/web ./apps/web
COPY packages/contracts ./packages/contracts

RUN corepack enable \
    && corepack prepare pnpm@10.33.0 --activate \
    && pnpm install --frozen-lockfile \
    && pnpm --filter @synapsegit-lp/web build \
    && mkdir -p /opt/lp-studio-license-corpus/node \
    && pnpm licenses list --prod --json \
        > /opt/lp-studio-license-corpus/pnpm-production-licenses.json \
    && find node_modules/.pnpm -type f \
        \( -iname 'LICENSE*' -o -iname 'COPYING*' -o -iname 'COPYRIGHT*' -o -iname 'NOTICE*' \) \
        -exec cp --parents '{}' /opt/lp-studio-license-corpus/node/ \;

FROM rust:1.95.0-slim-bookworm@sha256:d7482085ff5b415f84dba5647ae71606650bdef00db7aeb69f4b3d170c3e4082 AS rust-builder

WORKDIR /src

COPY Cargo.toml Cargo.lock ./
COPY apps/local-server ./apps/local-server
COPY templates ./templates

RUN cargo build --locked --release --package synapsegit-lp-local-server --bin synapsegit-lp-local-server \
    && mkdir -p /opt/lp-studio-license-corpus/rust \
    && cargo metadata --locked --format-version 1 \
        > /opt/lp-studio-license-corpus/cargo-metadata.json \
    && cd / \
    && find usr/local/cargo/registry/src usr/local/cargo/git/checkouts -type f \
        \( -iname 'LICENSE*' -o -iname 'COPYING*' -o -iname 'COPYRIGHT*' -o -iname 'NOTICE*' \) \
        -exec cp --parents '{}' /opt/lp-studio-license-corpus/rust/ \; \
    && synapse_license="$(find usr/local/cargo/git/checkouts -mindepth 3 -maxdepth 3 -type f -path '*/synapsegit-*/*/LICENSE' -print -quit)" \
    && test -n "$synapse_license" \
    && printf '%s  %s\n' \
        '200d6c727d7b3b62c85e7672c5615d21e5081fd95b8935acc5424bc98305415e' \
        "$synapse_license" | sha256sum -c - \
    && cp "$synapse_license" /opt/lp-studio-license-corpus/SynapseGit-v0.4.0-LICENSE

FROM debian:bookworm-slim@sha256:60eac759739651111db372c07be67863818726f754804b8707c90979bda511df AS runtime

ARG VERSION=local
ARG REVISION=unknown

LABEL org.opencontainers.image.title="SynapseGit LP Studio" \
      org.opencontainers.image.description="Local-only Docker build for SynapseGit LP Studio evaluation" \
      org.opencontainers.image.source="https://github.com/howlrs/synapsegit-lp-studio" \
      org.opencontainers.image.version="${VERSION}" \
      org.opencontainers.image.revision="${REVISION}" \
      org.opencontainers.image.licenses="LicenseRef-SynapseGit-LP-Studio-Evaluation"

COPY --from=rust-builder --chown=0:0 --chmod=0555 \
    /src/target/release/synapsegit-lp-local-server \
    /usr/local/bin/synapsegit-lp-local-server
COPY --from=web-builder --chown=65532:65532 /src/apps/web/dist /opt/synapsegit-lp-studio/web
COPY --from=rust-builder --chown=0:0 \
    /etc/ssl/certs/ca-certificates.crt \
    /etc/ssl/certs/ca-certificates.crt
COPY --from=rust-builder --chown=0:0 \
    /opt/lp-studio-license-corpus \
    /usr/share/licenses/synapsegit-lp-studio
COPY --from=web-builder --chown=0:0 \
    /opt/lp-studio-license-corpus \
    /usr/share/licenses/synapsegit-lp-studio
COPY --chown=0:0 LICENSE THIRD-PARTY-NOTICES.md /usr/share/licenses/synapsegit-lp-studio/

RUN install -d -o 65532 -g 65532 -m 0700 /var/lib/synapsegit-lp-studio/state \
    && install -d -o 65532 -g 65532 -m 0700 /run/secrets

ENV HOME=/tmp \
    SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt \
    LP_STUDIO_CONTAINER_MODE=1 \
    LP_STUDIO_EDITOR_PORT=4173 \
    LP_STUDIO_PREVIEW_PORT=4174 \
    LP_STUDIO_STATE_ROOT=/var/lib/synapsegit-lp-studio/state \
    LP_STUDIO_WEB_DIST=/opt/synapsegit-lp-studio/web

EXPOSE 4173 4174

WORKDIR /var/lib/synapsegit-lp-studio
USER 65532:65532
STOPSIGNAL SIGTERM

ENTRYPOINT ["/usr/local/bin/synapsegit-lp-local-server"]
