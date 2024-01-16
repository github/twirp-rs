#! /usr/bin/env bash
#/ Publishes twirp-rs to crates.io
#/ Usage: script/publish --version <version> --token <token> [--upload]
#/
#/ version: Version to publish to crates.io.
#/ token: crates.io token to use for publishing.
#/ upload: Actually publishes the the crate to crates.io. By default this script
#/         runs cargo publish in dry-run mode

set -euo pipefail
IFS=$'\n\t'

function usage {
    grep "^#/" "${BASH_SOURCE[0]}" | cut -c 4-
}


PKG_VERSION=$(cargo metadata --format-version 1 | jq  -r '.workspace_members[0]' | awk '{print $2}')
MODE="--dry-run"
TOKEN=
VERSION=

if [[ -z  "$(command -v jq)" ]]; then
    echo "jq is required to run this script"
    exit
fi


if [[  "$#" == 0 ]]
then
  usage
  exit
fi
# Ensure that the tests are still passing before publishing
cargo test --verbose

while [ $# -gt 0 ]; do
  case "$1" in
    --help|-h)
      usage; shift ;;
    --upload)
      MODE=; shift ;;
    --token)
      TOKEN=$2; shift 2;;
    --version)
      VERSION=$2; shift 2 ;;
    --)
      shift; break ;;
    *)
      usage
      exit
      ;;
   esac
done

if [[ -z "${VERSION}" ]]; then
    echo "Version is required"
    exit
fi

if [[ -z "${TOKEN}" ]]; then
    echo "Token is required"
    exit
fi

if [[ $(git rev-parse --abbrev-ref HEAD) != "main" ]]
then
   echo "You can only publish from main branch"
   exit
fi

if [[ "${PKG_VERSION}" != "${VERSION}" ]]; then
    echo "Version mismatched, cargo file: ${PKG_VERSION}, You supplied: ${VERSION}"
    exit
fi

cargo package

if [[ "${MODE}" == "--dry-run" ]]; then
    CMD="cargo publish --dry-run --token ${TOKEN}"
    eval $CMD
else
    git tag $VERSION
    CMD="cargo publish --token ${TOKEN}"
    echo "$CMD"
    eval $CMD
    git push origin $VERSION
fi
