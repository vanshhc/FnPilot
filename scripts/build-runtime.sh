#!/bin/bash
set -euo pipefail
project_dir="$(cd "$(dirname "$0")/.." && pwd)"
runtime_dir="${FNPILOT_RUNTIME_BUILD_DIR:-$project_dir/.build-runtime}"
mkdir -p "$runtime_dir"
if [ ! -d "$runtime_dir/source/.git" ]; then
    git clone --depth 1 --branch b11669 https://github.com/ggml-org/llama.cpp.git "$runtime_dir/source"
fi
expected_revision="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["revision"])' "$project_dir/scripts/runtime-manifest.json")"
test "$(git -C "$runtime_dir/source" rev-parse HEAD)" = "$expected_revision" || { echo 'Runtime revision mismatch'; exit 1; }
"${CMAKE:-cmake}" -S "$runtime_dir/source" -B "$runtime_dir/build" \
    -DCMAKE_BUILD_TYPE=Release -DLLAMA_BUILD_TESTS=OFF -DLLAMA_BUILD_EXAMPLES=OFF \
    -DLLAMA_BUILD_TOOLS=ON -DLLAMA_BUILD_SERVER=ON -DLLAMA_OPENSSL=OFF \
    -DGGML_METAL_EMBED_LIBRARY=ON -DGGML_NATIVE=OFF
"${CMAKE:-cmake}" --build "$runtime_dir/build" --target llama-server -j 2
python3 "$project_dir/scripts/package-runtime.py" "$runtime_dir/build/bin" --revision "$expected_revision"
