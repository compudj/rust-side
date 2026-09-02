#!/usr/bin/env bash

set -euo pipefail

resolve_tracer_path() {
    local lib_dirs=()
    local lib_dir_tokens=()
    local lib_name_tokens=()
    local lib_name
    local token
    local candidate

    read -r -a lib_dir_tokens <<< "$(pkg-config --libs-only-L lttng-ust)"
    for token in "${lib_dir_tokens[@]}"; do
        [[ "$token" == -L* ]] || continue
        lib_dirs+=("${token#-L}")
    done

    read -r -a lib_name_tokens <<< "$(pkg-config --libs-only-l lttng-ust)"
    for token in "${lib_name_tokens[@]}"; do
        [[ "$token" == -l* ]] || continue
        lib_name="${token#-l}"

        for dir in "${lib_dirs[@]}"; do
            for candidate in "$dir"/lib"$lib_name".so "$dir"/lib"$lib_name".so.*; do
                if [[ -r "$candidate" ]]; then
                    printf '%s\n' "$candidate"
                    return 0
                fi
            done
        done
    done

    return 1
}

if [[ $# -lt 1 ]]; then
    echo "expected a binary path from Cargo" >&2
    exit 1
fi

tracer_path="${LTTNG_UST_PATH:-}"

if [[ -z "$tracer_path" ]]; then
    if ! tracer_path="$(resolve_tracer_path)"; then
        echo "failed to locate liblttng-ust.so; set LTTNG_UST_PATH" >&2
        exit 1
    fi
fi

if [[ ! -r "$tracer_path" ]]; then
    echo "liblttng-ust is not readable: $tracer_path" >&2
    exit 1
fi

if [[ -n "${LD_PRELOAD:-}" ]]; then
    case ":$LD_PRELOAD:" in
        *":$tracer_path:"*) ;;
        *) export LD_PRELOAD="$tracer_path:$LD_PRELOAD" ;;
    esac
else
    export LD_PRELOAD="$tracer_path"
fi

exec "$@"
