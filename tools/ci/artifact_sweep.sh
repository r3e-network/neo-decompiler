#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

ARTIFACTS_DIR="TestingArtifacts"
DECOMPILED_DIR="$ARTIFACTS_DIR/decompiled"
KNOWN_UNSUPPORTED_FILE="$ARTIFACTS_DIR/known_unsupported.txt"
BIN_PATH="${BIN_PATH:-$ROOT_DIR/target/debug/neo-decompiler}"

TMP_DIR="$(mktemp -d)"
FAILURES_FILE="$TMP_DIR/failures.log"
touch "$FAILURES_FILE"

cleanup() {
    rm -rf "$TMP_DIR"
}
trap cleanup EXIT

log() {
    printf '[artifact-sweep] %s\n' "$*"
}

trim() {
    local value="$1"
    value="${value#"${value%%[![:space:]]*}"}"
    value="${value%"${value##*[![:space:]]}"}"
    printf '%s' "$value"
}

record_failure() {
    local stage="$1"
    local id="$2"
    local err_file="$3"
    {
        printf '[%s] %s\n' "$stage" "$id"
        if [[ -s "$err_file" ]]; then
            sed 's/^/    /' "$err_file"
        else
            printf '    (no error output captured)\n'
        fi
    } >>"$FAILURES_FILE"
}

hash_decompiled_outputs() {
    local files=()
    if [[ -d "$DECOMPILED_DIR" ]]; then
        while IFS= read -r -d '' file; do
            files+=("$file")
        done < <(find "$DECOMPILED_DIR" -type f \
            \( -name '*.high-level.cs' -o -name '*.pseudocode.txt' -o -name '*.error.txt' -o -name '*.nef' -o -name '*.manifest.json' \) \
            -print0 | sort -z)
    fi

    if (( ${#files[@]} == 0 )); then
        printf 'empty'
        return
    fi

    sha256sum "${files[@]}" | sha256sum | awk '{print $1}'
}

declare -A KNOWN_EXPECTED
declare -A KNOWN_SEEN

if [[ -f "$KNOWN_UNSUPPORTED_FILE" ]]; then
    while IFS= read -r raw_line || [[ -n "$raw_line" ]]; do
        raw_line="${raw_line%%#*}"
        raw_line="$(trim "$raw_line")"
        if [[ -z "$raw_line" ]]; then
            continue
        fi

        local_id="$raw_line"
        local_expected=""
        if [[ "$raw_line" == *:* ]]; then
            local_id="$(trim "${raw_line%%:*}")"
            local_expected="$(trim "${raw_line#*:}")"
        fi

        KNOWN_EXPECTED["$local_id"]="$local_expected"
    done < "$KNOWN_UNSUPPORTED_FILE"
fi

known_key_for_id() {
    local id="$1"
    local basename="${id##*/}"

    if [[ -n "${KNOWN_EXPECTED[$id]+_}" ]]; then
        printf '%s' "$id"
        return 0
    fi

    if [[ -n "${KNOWN_EXPECTED[$basename]+_}" ]]; then
        printf '%s' "$basename"
        return 0
    fi

    return 1
}

declare -A ARTIFACT_PATHS
declare -A ARTIFACT_MANIFESTS

add_artifact_pair() {
    local id="$1"
    local nef_path="$2"
    local manifest_path="$3"
    ARTIFACT_PATHS["$id"]="$nef_path"
    ARTIFACT_MANIFESTS["$id"]="$manifest_path"
}

log "Regenerating artifact outputs (run 1)"
cargo test --locked --test decompile_artifacts -- --nocapture >/dev/null
first_hash="$(hash_decompiled_outputs)"

log "Regenerating artifact outputs (run 2)"
cargo test --locked --test decompile_artifacts -- --nocapture >/dev/null
second_hash="$(hash_decompiled_outputs)"

if [[ "$first_hash" != "$second_hash" ]]; then
    printf '[DRIFT] decompiled output hash mismatch\n' >>"$FAILURES_FILE"
    printf '    first:  %s\n' "$first_hash" >>"$FAILURES_FILE"
    printf '    second: %s\n' "$second_hash" >>"$FAILURES_FILE"
fi

log "Building CLI binary"
cargo build --locked --quiet

if [[ ! -x "$BIN_PATH" ]]; then
    printf '[SETUP] binary missing or not executable: %s\n' "$BIN_PATH" >>"$FAILURES_FILE"
fi

while IFS= read -r -d '' nef_path; do
    manifest_path="${nef_path%.nef}.manifest.json"
    if [[ -f "$manifest_path" ]]; then
        id="${nef_path#$ARTIFACTS_DIR/}"
        id="${id%.nef}"
        add_artifact_pair "$id" "$nef_path" "$manifest_path"
    fi
done < <(find "$ARTIFACTS_DIR" -type f -name '*.nef' ! -path "$DECOMPILED_DIR/*" -print0 | sort -z)

if [[ -d "$DECOMPILED_DIR" ]]; then
    while IFS= read -r -d '' nef_path; do
        manifest_path="${nef_path%.nef}.manifest.json"
        if [[ -f "$manifest_path" ]]; then
            id="${nef_path#$DECOMPILED_DIR/}"
            id="${id%.nef}"
            add_artifact_pair "$id" "$nef_path" "$manifest_path"
        fi
    done < <(find "$DECOMPILED_DIR" -type f -name '*.nef' -print0 | sort -z)
fi

if (( ${#ARTIFACT_PATHS[@]} == 0 )); then
    printf '[SETUP] no artifacts discovered\n' >>"$FAILURES_FILE"
fi

tmp_for() {
    local id="$1"
    local suffix="$2"
    local file="$TMP_DIR/$id.$suffix"
    mkdir -p "$(dirname "$file")"
    printf '%s' "$file"
}

validate_schema() {
    local schema_kind="$1"
    local payload_path="$2"
    local schema_err="$3"
    "$BIN_PATH" schema "$schema_kind" --validate "$payload_path" --no-print >/dev/null 2>"$schema_err"
}

pairs_total=0
pairs_full_success=0
pairs_known_fail=0
pairs_unexpected_fail=0

info_ok=0
disasm_ok=0
decompile_ok=0
tokens_ok=0
schema_info_ok=0
schema_disasm_ok=0
schema_decompile_ok=0
schema_tokens_ok=0

for id in "${!ARTIFACT_PATHS[@]}"; do
    pairs_total=$((pairs_total + 1))
    nef_path="${ARTIFACT_PATHS[$id]}"
    manifest_path="${ARTIFACT_MANIFESTS[$id]}"

    known_key=""
    if known_key="$(known_key_for_id "$id")"; then
        KNOWN_SEEN["$known_key"]=1
    fi
    expected_hint=""
    if [[ -n "$known_key" ]]; then
        expected_hint="${KNOWN_EXPECTED[$known_key]}"
    fi

    pair_failed=0
    decompile_failed=0

    info_out="$(tmp_for "$id" info.json)"
    info_err="$(tmp_for "$id" info.err)"
    if "$BIN_PATH" --manifest "$manifest_path" info --format json "$nef_path" >"$info_out" 2>"$info_err"; then
        info_ok=$((info_ok + 1))
        schema_err="$(tmp_for "$id" schema_info.err)"
        if validate_schema info "$info_out" "$schema_err"; then
            schema_info_ok=$((schema_info_ok + 1))
        else
            pair_failed=1
            record_failure "SCHEMA-INFO" "$id" "$schema_err"
        fi
    else
        if [[ -z "$known_key" ]]; then
            pair_failed=1
            record_failure "INFO" "$id" "$info_err"
        fi
    fi

    disasm_out="$(tmp_for "$id" disasm.json)"
    disasm_err="$(tmp_for "$id" disasm.err)"
    if "$BIN_PATH" disasm --format json "$nef_path" >"$disasm_out" 2>"$disasm_err"; then
        disasm_ok=$((disasm_ok + 1))
        schema_err="$(tmp_for "$id" schema_disasm.err)"
        if validate_schema disasm "$disasm_out" "$schema_err"; then
            schema_disasm_ok=$((schema_disasm_ok + 1))
        else
            pair_failed=1
            record_failure "SCHEMA-DISASM" "$id" "$schema_err"
        fi
    else
        if [[ -z "$known_key" ]]; then
            pair_failed=1
            record_failure "DISASM" "$id" "$disasm_err"
        fi
    fi

    decompile_out="$(tmp_for "$id" decompile.json)"
    decompile_err="$(tmp_for "$id" decompile.err)"
    if "$BIN_PATH" --manifest "$manifest_path" decompile --format json "$nef_path" >"$decompile_out" 2>"$decompile_err"; then
        if [[ -n "$known_key" ]]; then
            pair_failed=1
            record_failure "KNOWN-DECOMPILE-SUCCEEDED" "$id" "$decompile_err"
        else
            decompile_ok=$((decompile_ok + 1))
            schema_err="$(tmp_for "$id" schema_decompile.err)"
            if validate_schema decompile "$decompile_out" "$schema_err"; then
                schema_decompile_ok=$((schema_decompile_ok + 1))
            else
                pair_failed=1
                record_failure "SCHEMA-DECOMPILE" "$id" "$schema_err"
            fi
        fi
    else
        decompile_failed=1
        if [[ -n "$known_key" ]]; then
            if [[ -n "$expected_hint" ]] && ! grep -Fq "$expected_hint" "$decompile_err"; then
                pair_failed=1
                record_failure "KNOWN-DECOMPILE-MISMATCH" "$id" "$decompile_err"
            fi
        else
            pair_failed=1
            record_failure "DECOMPILE" "$id" "$decompile_err"
        fi
    fi

    tokens_out="$(tmp_for "$id" tokens.json)"
    tokens_err="$(tmp_for "$id" tokens.err)"
    if "$BIN_PATH" tokens --format json "$nef_path" >"$tokens_out" 2>"$tokens_err"; then
        tokens_ok=$((tokens_ok + 1))
        schema_err="$(tmp_for "$id" schema_tokens.err)"
        if validate_schema tokens "$tokens_out" "$schema_err"; then
            schema_tokens_ok=$((schema_tokens_ok + 1))
        else
            pair_failed=1
            record_failure "SCHEMA-TOKENS" "$id" "$schema_err"
        fi
    else
        if [[ -z "$known_key" ]]; then
            pair_failed=1
            record_failure "TOKENS" "$id" "$tokens_err"
        fi
    fi

    if [[ -n "$known_key" ]] && [[ "$decompile_failed" -eq 0 ]]; then
        pair_failed=1
        printf '[KNOWN-DECOMPILE-SUCCEEDED] %s\n    expected decompile failure for known unsupported entry\n' "$id" >>"$FAILURES_FILE"
    fi

    if [[ "$pair_failed" -eq 0 ]]; then
        if [[ -n "$known_key" ]]; then
            pairs_known_fail=$((pairs_known_fail + 1))
        else
            pairs_full_success=$((pairs_full_success + 1))
        fi
    else
        pairs_unexpected_fail=$((pairs_unexpected_fail + 1))
    fi
done

for key in "${!KNOWN_EXPECTED[@]}"; do
    if [[ -z "${KNOWN_SEEN[$key]+_}" ]]; then
        printf '[STALE-KNOWN-UNSUPPORTED] %s\n    listed in %s but no matching artifact id found\n' "$key" "$KNOWN_UNSUPPORTED_FILE" >>"$FAILURES_FILE"
    fi
done

printf 'ARTIFACT_SWEEP\n'
printf 'pairs=%d\n' "$pairs_total"
printf 'pairs_full_success=%d\n' "$pairs_full_success"
printf 'pairs_known_expected_failure=%d\n' "$pairs_known_fail"
printf 'pairs_unexpected_failure=%d\n' "$pairs_unexpected_fail"
printf 'info_ok=%d\n' "$info_ok"
printf 'disasm_ok=%d\n' "$disasm_ok"
printf 'decompile_ok=%d\n' "$decompile_ok"
printf 'tokens_ok=%d\n' "$tokens_ok"
printf 'schema_info_ok=%d\n' "$schema_info_ok"
printf 'schema_disasm_ok=%d\n' "$schema_disasm_ok"
printf 'schema_decompile_ok=%d\n' "$schema_decompile_ok"
printf 'schema_tokens_ok=%d\n' "$schema_tokens_ok"
printf 'determinism_hash_run1=%s\n' "$first_hash"
printf 'determinism_hash_run2=%s\n' "$second_hash"

if [[ -s "$FAILURES_FILE" ]]; then
    echo '--- FAILURES ---'
    cat "$FAILURES_FILE"
    exit 1
fi

echo '--- FAILURES ---'
echo '(none)'
