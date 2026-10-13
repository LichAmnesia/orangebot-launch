#!/usr/bin/env bash
# Build the sidecar binaries that Tauri bundles via bundle.externalBin.
set -euo pipefail

LLAMA_TAG=b11570
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TRIPLE="$(rustc -vV | sed -n 's/^host: //p')"
OUT="$ROOT/apps/desktop/binaries"
SRC="$ROOT/target/sidecars/llama.cpp-$LLAMA_TAG"

if [ ! -d "$SRC/.git" ]; then
  git clone --depth 1 --branch "$LLAMA_TAG" https://github.com/ggml-org/llama.cpp "$SRC"
fi

rm -f "$SRC/build/CMakeCache.txt"
cmake -S "$SRC" -B "$SRC/build" \
  -DCMAKE_BUILD_TYPE=Release \
  -DBUILD_SHARED_LIBS=OFF \
  -DLLAMA_CURL=OFF \
  -DLLAMA_OPENSSL=OFF \
  -DLLAMA_BUILD_TESTS=OFF \
  -DLLAMA_BUILD_EXAMPLES=OFF \
  -DLLAMA_BUILD_SERVER=ON \
  -DGGML_METAL=ON \
  -DGGML_METAL_EMBED_LIBRARY=ON \
  -DGGML_NATIVE=OFF

cmake --build "$SRC/build" --target llama-server -j 4

cargo build --release -p ob-asr-server

mkdir -p "$OUT"
cp "$SRC/build/bin/llama-server" "$OUT/llama-server-$TRIPLE"
cp "$ROOT/target/release/ob-asr-server" "$OUT/ob-asr-server-$TRIPLE"
chmod +x "$OUT"/*

echo "$OUT/llama-server-$TRIPLE"
echo "$OUT/ob-asr-server-$TRIPLE"

echo "non-system linked libraries of llama-server (expect none):"
BAD=0
while read -r lib; do
  case "$lib" in
    /System* | /usr/lib*) ;;
    /opt/homebrew* | /usr/local*)
      echo "$lib"
      BAD=1
      ;;
    *) echo "$lib" ;;
  esac
done < <(otool -L "$OUT/llama-server-$TRIPLE" | tail -n +2 | awk '{print $1}')
if [ "$BAD" -ne 0 ]; then
  echo "error: llama-server links Homebrew/local libraries; binary is not relocatable" >&2
  exit 1
fi
