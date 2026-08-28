#!/bin/sh
set -eu

COMMIT="8c278562ebac7334d842723b6508d613d6019637"
URL="https://raw.githubusercontent.com/adobe-type-tools/mapping-resources-pdf/${COMMIT}/pdf2unicode/Adobe-Japan1-UCS2"
SHA256="6a9693361647a37996312cc57071bb79f8c06411207be7c730a83fda1254cd82"
TARGET="$(dirname "$0")/../data/Adobe-Japan1-UCS2"

mkdir -p "$(dirname "$TARGET")"

if [ -f "$TARGET" ] && echo "$SHA256  $TARGET" | sha256sum -c - >/dev/null 2>&1; then
  exit 0
fi

curl --fail --silent --show-error --location "$URL" --output "$TARGET"
echo "$SHA256  $TARGET" | sha256sum -c -
