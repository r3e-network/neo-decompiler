#!/usr/bin/env bash
# Run every fuzz target for a fixed budget and report a verdict per target.
#
# The script is the local counterpart of the CI fuzz job. It runs each
# target with the same sanitizer-instrumented binary that CI uses, keeps the
# corpus between runs, and reports which targets found a crash.
#
# Usage:
#   scripts/fuzz.sh [seconds-per-target] [target ...]
#
# With no target names, every target in fuzz/Cargo.toml is run.

set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fuzz_dir="${repo_root}/fuzz"

seconds="${1:-60}"
shift || true

# The run log lives outside the artifacts directory. libFuzzer writes the
# crashing input next to the artifacts, and a log file sitting in that
# directory is picked up as a crash input by the next run.
log_dir="${FUZZ_LOG_DIR:-${fuzz_dir}/logs}"
mkdir -p "${log_dir}"

# Read the target list from the fuzz manifest so a new target is picked up
# automatically instead of needing a second edit here.
if [[ $# -gt 0 ]]; then
    targets=("$@")
else
    mapfile -t targets < <(
        sed -n 's/^name = "\(fuzz_[a-z_]*\)"$/\1/p' "${fuzz_dir}/Cargo.toml"
    )
fi

if [[ ${#targets[@]} -eq 0 ]]; then
    echo "no fuzz targets found in ${fuzz_dir}/Cargo.toml" >&2
    exit 1
fi

# The sanitizer runtime needs a writable target directory and the corpus
# must persist between runs so coverage accumulates.
target_dir="${FUZZ_TARGET_DIR:-${fuzz_dir}/target}"
corpus_root="${fuzz_dir}/corpus"
artifacts_root="${fuzz_dir}/artifacts"

echo "=== Fuzzing ${#targets[@]} target(s), ${seconds}s each ==="

failed=0
for target in "${targets[@]}"; do
    corpus="${corpus_root}/${target}"
    artifacts="${artifacts_root}/${target}"
    mkdir -p "${corpus}" "${artifacts}"

    echo "--- ${target} ---"
    start=$(date +%s)
    # `-max_total_time` makes the run bounded; the timeout is a backstop for
    # a target that hangs before the fuzzer takes over.
    #
    # The artifact prefix is set explicitly rather than passing the
    # artifacts directory as a second corpus directory. cargo-fuzz turns
    # the last corpus directory into the artifact prefix, and a directory
    # that holds crash files would then be read back as seed inputs -- so
    # a crash that has already been fixed keeps being replayed forever.
    (
        cd "${fuzz_dir}" &&
            RUSTUP_TOOLCHAIN="${RUSTUP_TOOLCHAIN:-nightly}" \
            CARGO_TARGET_DIR="${target_dir}" \
            cargo fuzz run "${target}" "${corpus}" -- \
                -max_total_time="${seconds}" -print_final_stats=1 \
                -artifact_prefix="${artifacts}/"
    ) >"${log_dir}/${target}.log" 2>&1
    status=$?
    elapsed=$(( $(date +%s) - start ))

    if [[ ${status} -eq 0 ]]; then
        units=$(sed -n 's/^stat::number_of_executed_units: *//p' "${log_dir}/${target}.log" | tail -1)
        added=$(sed -n 's/^stat::new_units_added: *//p' "${log_dir}/${target}.log" | tail -1)
        echo "    ok    ${elapsed}s  units=${units:-?}  new_corpus=${added:-?}"
    else
        echo "    FAIL  ${elapsed}s  (exit ${status})"
        # The crash file is the only thing that matters for a failure.
        ls -1 "${artifacts}" | head -5 | sed 's/^/          /'
        tail -25 "${log_dir}/${target}.log" | sed 's/^/          /'
        failed=$((failed + 1))
    fi
done

# The mutation-based target mutates real contracts. A run without the
# contract corpus loads nothing and tests nothing, so the corpus size is
# reported rather than left implicit.
corpus_size=$(sed -n 's/^corpus size: *\([0-9]*\).*/\1/p' "${log_dir}/fuzz_corpus_mutation.log" 2>/dev/null | tail -1)
if [[ -n "${corpus_size}" ]]; then
    echo "=== contract corpus: ${corpus_size} contract(s) ==="
else
    echo "=== contract corpus: none found (the mutation target tested nothing) ==="
fi

if [[ ${failed} -gt 0 ]]; then
    echo "=== ${failed} target(s) reported a crash ==="
    exit 1
fi

echo "=== all ${#targets[@]} target(s) completed without a crash ==="
