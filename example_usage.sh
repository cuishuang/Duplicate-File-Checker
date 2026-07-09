#!/usr/bin/env bash
set -euo pipefail

echo "Building find-dupl-file..."
cargo build

echo "Starting CLI mode..."
echo "Inside the CLI you can run commands such as:"
echo "  scan ./demo_files demo"
echo "  find"
echo "  export"
echo "  stats"
echo "  exit"
echo

cargo run -- --cli
