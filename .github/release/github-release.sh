#!/usr/bin/env bash
# github-release.sh <dmg> <notes>: mirrors a published stable release to
# GitHub: creates the release for FURITEN_RELEASE_TAG (which tag-release.sh
# pushed) with the hand-written notes and the DMG. A rerun leaves an existing
# release alone. Runs gh only, with the job's GITHUB_TOKEN.
. "$(dirname "$0")/lib.sh"

dmg=$1
notes=$2
: "${FURITEN_RELEASE_TAG:?Set FURITEN_RELEASE_TAG}"
: "${FURITEN_VERSION:?Set FURITEN_VERSION}"
[ -s "$notes" ] || die "Missing release notes $notes."
if gh release view "$FURITEN_RELEASE_TAG" --repo "$GITHUB_REPOSITORY" > /dev/null 2>&1; then
  echo "GitHub release $FURITEN_RELEASE_TAG already exists."
  exit 0
fi
staged=$(mktemp -d)
trap 'rm -rf "$staged"' EXIT
cp "$dmg" "$staged/Furiten-$FURITEN_VERSION.dmg"
gh release create "$FURITEN_RELEASE_TAG" --repo "$GITHUB_REPOSITORY" --verify-tag \
  --title "Furiten $FURITEN_VERSION" --notes-file "$notes" "$staged/Furiten-$FURITEN_VERSION.dmg"
