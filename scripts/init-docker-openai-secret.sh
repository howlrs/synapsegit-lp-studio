#!/usr/bin/env bash
set -euo pipefail

readonly image_name="synapsegit-lp-studio:local"
readonly secret_volume="${LP_STUDIO_OPENAI_SECRET_VOLUME:-synapsegit-lp-studio-openai-secret}"

if [[ ! "$secret_volume" =~ ^[A-Za-z0-9][A-Za-z0-9_.-]*$ ]]; then
  printf 'Invalid Docker volume name: %s\n' "$secret_volume" >&2
  exit 1
fi

if ! docker image inspect "$image_name" </dev/null >/dev/null 2>&1; then
  printf 'Build the local image first: docker compose build\n' >&2
  exit 1
fi

openai_api_key=""
trap 'unset openai_api_key' EXIT

if ! IFS= read -r -s -p 'OpenAI API key: ' openai_api_key; then
  printf '\nNo credential was read.\n' >&2
  exit 1
fi
printf '\n' >&2

if [[ -z "$openai_api_key" ]]; then
  printf 'The OpenAI API key must not be empty.\n' >&2
  exit 1
fi
if (( ${#openai_api_key} > 1024 )); then
  printf 'The OpenAI API key exceeds the 1024-byte application limit.\n' >&2
  exit 1
fi

docker volume create "$secret_volume" >/dev/null

printf '%s' "$openai_api_key" | docker run --rm --interactive \
  --read-only \
  --network none \
  --pids-limit 32 \
  --user 65532:65532 \
  --cap-drop ALL \
  --security-opt no-new-privileges \
  --mount "type=volume,src=${secret_volume},dst=/run/secrets" \
  --entrypoint /bin/sh \
  "$image_name" \
  -c '
    set -eu
    umask 077
    secret_tmp=/run/secrets/.openai-api-key.tmp
    trap '\''rm -f "$secret_tmp"'\'' 0 1 2 15
    cat > "$secret_tmp"
    secret_size="$(wc -c < "$secret_tmp")"
    test "$secret_size" -ge 1
    test "$secret_size" -le 1024
    chmod 600 "$secret_tmp"
    mv -f "$secret_tmp" /run/secrets/openai-api-key
  '

unset openai_api_key
trap - EXIT

printf 'Credential stored in Docker volume %s.\n' "$secret_volume"
printf '%s\n' 'Start with: docker compose -f compose.yaml -f compose.openai.yaml up'
