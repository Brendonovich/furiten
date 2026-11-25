#!/usr/bin/env bash
# plan.sh: decides what this run builds and where it publishes, from the
# event, the ref, the crates' version and the v* tags alone, and writes the
# plan to $GITHUB_OUTPUT.
#
#   dev    a push to dev (or a dispatch from dev): Furiten Dev, published on
#          the release service's dev channel as 0.0.0-dev-<run number>.
#   check  a dispatch from dev: the dev build, signed, notarized and verified,
#          published nowhere. The tag job pushes and deletes a throwaway
#          v0.0.0-test-<run> tag to prove it can tag.
#   stable a dispatch from dev: Furiten on the stable channel. The version is
#          BUMP (patch, minor or major) past the newest vX.Y.Z tag, or the
#          crates' version when that is newer or there is no tag yet. Needs
#          changelog/<version>.md; the workflow tags v<version> only after it
#          published.
#
# The version is display text: installs follow the channel and never compare
# versions. The build stamps it into the crates and Info.plist, so the app
# knows it is that build.
#
# A run releases the commit it runs on: SHA (the checked-out commit) must be
# GITHUB_SHA, so every job can check out github.sha.
#
# Environment: EVENT, REF_NAME, CHANNEL, BUMP, SHA, GITHUB_SHA,
# GITHUB_RUN_NUMBER, GITHUB_REPOSITORY, GH_TOKEN, and Cargo.toml and
# changelog/ of the commit.
. "$(dirname "$0")/lib.sh"

[ -n "$SHA" ] && [ "$SHA" = "${GITHUB_SHA:-}" ] || die "The checked-out commit $SHA is not this run's commit ${GITHUB_SHA:-}."
[ "$REF_NAME" = dev ] || die "Releases run from dev, not $REF_NAME."

semver_max() { sort -t. -k1,1n -k2,2n -k3,3n | tail -n 1; }

tag=""
case "$CHANNEL" in
  stable)
    [ "$EVENT" = workflow_dispatch ] || die "Stable releases are dispatched."
    crate=$(awk -F ' *= *' '
      /^\[workspace\.package\]$/ { package = 1; next }
      /^\[/ { package = 0 }
      package && $1 == "version" { gsub(/"/, "", $2); print $2; exit }
    ' Cargo.toml)
    [[ "$crate" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "Cargo.toml's version must look like 1.2.3, not '$crate'."
    latest=$(gh api --paginate "repos/$GITHUB_REPOSITORY/git/matching-refs/tags/v" \
      --jq '.[].ref | ltrimstr("refs/tags/v") | select(test("^[0-9]+\\.[0-9]+\\.[0-9]+$"))' | semver_max)
    if [ -z "$latest" ] || [ "$(printf '%s\n%s\n' "$crate" "$latest" | semver_max)" != "$latest" ]; then
      # No release yet, or Cargo.toml was raised past the newest tag: release that version.
      version=$crate
    else
      IFS=. read -r major minor patch <<< "$latest"
      case "$BUMP" in
        major) version="$((major + 1)).0.0" ;;
        minor) version="$major.$((minor + 1)).0" ;;
        patch) version="$major.$minor.$((patch + 1))" ;;
        *) die "Unknown bump $BUMP." ;;
      esac
    fi
    tag="v$version"
    refs=$(gh api "repos/$GITHUB_REPOSITORY/git/matching-refs/tags/$tag" --jq '.[].ref')
    if grep -Fxq "refs/tags/$tag" <<< "$refs"; then die "Furiten $version is already tagged $tag."; fi
    [ -s "changelog/$version.md" ] || die "Add release notes at changelog/$version.md first."
    release_channel=stable
    build_channel=production
    app_name=Furiten
    bundle_id=co.anomaly.furiten
    notes="https://github.com/$GITHUB_REPOSITORY/releases/tag/$tag"
    ;;
  dev | check)
    [ "$CHANNEL" = dev ] || [ "$EVENT" = workflow_dispatch ] || die "Check runs are dispatched."
    version="0.0.0-dev-$GITHUB_RUN_NUMBER"
    release_channel=dev
    build_channel=dev
    app_name="Furiten Dev"
    bundle_id=co.anomaly.furiten.dev
    notes="https://github.com/$GITHUB_REPOSITORY/commit/$SHA"
    if [ "$CHANNEL" = check ]; then tag="v0.0.0-test-$GITHUB_RUN_NUMBER"; fi
    ;;
  *) die "Unknown channel $CHANNEL." ;;
esac
{
  echo "version=$version"
  echo "channel=$CHANNEL"
  echo "release-channel=$release_channel"
  echo "build-channel=$build_channel"
  echo "app-name=$app_name"
  echo "bundle-id=$bundle_id"
  echo "service=https://anoma.ly/release"
  echo "tag=$tag"
  echo "notes-url=$notes"
} >> "$GITHUB_OUTPUT"
echo "Releasing $app_name $version ($CHANNEL) from $SHA." >> "$GITHUB_STEP_SUMMARY"
