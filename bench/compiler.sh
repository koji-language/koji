#!/bin/sh
# The benchmark of the optimizations (phase 6 of doc/compiler-plan.md): the compiler working on
# its own source (about 15000 lines). Prints the best time of five runs of each stage and, with
# valgrind installed, the instructions of `check` (exact, unlike times).
#
#     bench/compiler.sh [kojic]      (default: build/kojic, from bootstrap/build.sh)
set -eu

cd "$(dirname "$0")/.."
kojic=${1:-build/kojic}

for stage in "check" "dump ir" "dump c"; do
    best=""
    for run in 1 2 3 4 5; do
        start=$(date +%s.%N)
        # shellcheck disable=SC2086
        "$kojic" $stage compiler/main.kj > /dev/null
        end=$(date +%s.%N)
        time=$(echo "$end - $start" | bc)
        if [ -z "$best" ] || [ "$(echo "$time < $best" | bc)" = 1 ]; then best=$time; fi
    done
    printf '%-8s %6.3f s\n' "$stage" "$best"
done

if command -v valgrind > /dev/null; then
    out=$(mktemp)
    valgrind --tool=callgrind --callgrind-out-file="$out" "$kojic" check compiler/main.kj 2>&1 | sed -n 's/.*refs: *//p' | sed 's/^/check    instructions: /'
    rm -f "$out"
fi
