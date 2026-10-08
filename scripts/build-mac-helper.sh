#!/bin/sh
# Builds electron/mac/media-helper as one universal binary (Apple Silicon +
# Intel). Run on a Mac with the Xcode Command Line Tools (xcode-select --install).
set -e
cd "$(dirname "$0")/../electron/mac"
for arch in arm64 x86_64; do
  swiftc -O -swift-version 5 -target "$arch-apple-macos11" media-helper.swift -o "media-helper-$arch"
done
lipo -create media-helper-arm64 media-helper-x86_64 -output media-helper
rm media-helper-arm64 media-helper-x86_64
echo "built electron/mac/media-helper"
