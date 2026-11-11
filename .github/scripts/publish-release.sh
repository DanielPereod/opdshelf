#!/usr/bin/env bash
# Moves a release tag to this commit and replaces the release's file with a
# new build, creating the release the first time.
# Usage: publish-release.sh <tag> <title> <file>
set -euo pipefail
tag="$1"
title="$2"
file="$3"
beta=()
if [[ "$tag" == *-beta ]]; then
  title="$title (beta)"
  beta=(--prerelease)
fi
notes="Latest build, from ${GITHUB_SHA:0:7}. OPDShelf servers link to this release in the web panel (Devices → Connect your apps)."

git tag -f "$tag"
git push -f origin "refs/tags/$tag"
if gh release view "$tag" >/dev/null 2>&1; then
  gh release upload "$tag" "$file" --clobber
  gh release edit "$tag" --title "$title" --notes "$notes" "${beta[@]}"
else
  gh release create "$tag" "$file" --title "$title" --notes "$notes" --latest=false "${beta[@]}"
fi
