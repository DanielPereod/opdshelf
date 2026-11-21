#!/usr/bin/env bash
# Waits for the CI run of this commit (workflow ci.yml, push event) and fails unless it
# passed, so what gets published (the image, the APK, the KOReader plugin that updates
# itself on e-readers) never comes from a commit whose tests failed.
set -euo pipefail

: "${GH_TOKEN:?}" "${GITHUB_REPOSITORY:?}" "${GITHUB_SHA:?}"

# Up to 90 minutes, looking every 30 seconds
for _ in $(seq 1 180); do
  run=$(gh run list --repo "$GITHUB_REPOSITORY" --workflow ci.yml --commit "$GITHUB_SHA" --event push \
    --json status,conclusion,url --limit 1 --jq '.[0] // empty')
  if [ -n "$run" ]; then
    status=$(jq -r .status <<<"$run")
    conclusion=$(jq -r .conclusion <<<"$run")
    url=$(jq -r .url <<<"$run")
    if [ "$status" = "completed" ]; then
      if [ "$conclusion" = "success" ]; then
        echo "CI passed for $GITHUB_SHA: $url"
        exit 0
      fi
      echo "::error::CI did not pass for $GITHUB_SHA ($conclusion): $url. Nothing is published."
      exit 1
    fi
  fi
  sleep 30
done
echo "::error::Gave up waiting for the CI of $GITHUB_SHA. Nothing is published."
exit 1
