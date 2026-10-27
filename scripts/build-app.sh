#!/bin/bash
set -euo pipefail
project_dir="$(cd "$(dirname "$0")/.." && pwd)"
configuration="${FNPILOT_CONFIGURATION:-release}"
build_dir="${FNPILOT_BUILD_DIR:-$project_dir/.build}"
app_path="${1:-$project_dir/../FnPilot.app}"
python3 "$project_dir/scripts/sign-app.py" --check
swift build --package-path "$project_dir" --scratch-path "$build_dir" -c "$configuration" --jobs 2 --product FnPilot
swift build --package-path "$project_dir" --scratch-path "$build_dir" -c "$configuration" --jobs 2 --product fnpilot-cli
bin_path="$(swift build --package-path "$project_dir" --scratch-path "$build_dir" -c "$configuration" --show-bin-path)"
mkdir -p "$app_path/Contents/MacOS" "$app_path/Contents/Resources"
cp "$bin_path/FnPilot" "$app_path/Contents/MacOS/FnPilot"
cp "$bin_path/fnpilot-cli" "$app_path/Contents/MacOS/fnpilot-cli"
cp "$project_dir/Resources/Info.plist" "$app_path/Contents/Info.plist"
cp "$project_dir/THIRD_PARTY_NOTICES.txt" "$app_path/Contents/Resources/THIRD_PARTY_NOTICES.txt"
for resource in "$bin_path"/*.bundle; do
    if [ -d "$resource" ]; then cp -R "$resource" "$app_path/Contents/Resources/"; fi
done
if [ -d "$project_dir/Resources/Runtime" ]; then
    mkdir -p "$app_path/Contents/Resources/Runtime"
    cp -R "$project_dir/Resources/Runtime/." "$app_path/Contents/Resources/Runtime/"
fi
python3 "$project_dir/scripts/sign-app.py" "$app_path"
printf 'Built %s\n' "$app_path"
