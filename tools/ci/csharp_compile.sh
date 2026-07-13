#!/usr/bin/env bash
set -euo pipefail

command -v dotnet >/dev/null || { echo "dotnet is required" >&2; exit 1; }
: "${NEO_SMARTCONTRACT_FRAMEWORK_DLL:?NEO_SMARTCONTRACT_FRAMEWORK_DLL is required}"

if [[ ! -f "$NEO_SMARTCONTRACT_FRAMEWORK_DLL" ]]; then
    echo "NEO_SMARTCONTRACT_FRAMEWORK_DLL does not point to a file: $NEO_SMARTCONTRACT_FRAMEWORK_DLL" >&2
    exit 1
fi

if [[ -n "${NEO_CSHARP_TARGET_FRAMEWORK:-}" ]] &&
    [[ ! "$NEO_CSHARP_TARGET_FRAMEWORK" =~ ^net[[:alnum:].]+$ ]]; then
    echo "NEO_CSHARP_TARGET_FRAMEWORK must be a target moniker such as net8.0" >&2
    exit 1
fi

cargo test --locked --test csharp_compile -- --ignored --nocapture
