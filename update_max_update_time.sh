#!/bin/bash

set -euo pipefail

if [ "$#" -lt 3 ] || [ "$#" -gt 4 ]; then
    echo "Usage: $0 <package_id> <config_object_id> <max_update_time_ms> [gas_budget]"
    echo "Example: $0 0xef1c09c6167fb6d3471bc517d2e0e5427b0a759c205e806adc9d184f871dc2f0 \\"
    echo "             0x50798ba933cff16561667108d9ccf4d9ebd8f6538503fc7d3e94a40502529faa 60000"
    exit 1
fi

PACKAGE_ID=$1
CONFIG_OBJECT_ID=$2
MAX_UPDATE_TIME_MS=$3
GAS_BUDGET=${4:-100000000}

# Ensure config object id has 0x prefix
if [[ "$CONFIG_OBJECT_ID" =~ ^0x ]]; then
    CONFIG_ARG="@${CONFIG_OBJECT_ID}"
else
    CONFIG_ARG="@0x${CONFIG_OBJECT_ID}"
fi

echo "Updating max_update_time_ms to $MAX_UPDATE_TIME_MS for config object $CONFIG_OBJECT_ID"

sui client ptb \
    --move-call "${PACKAGE_ID}::config::update_max_update_time_ms" \
        "$CONFIG_ARG" \
        "$MAX_UPDATE_TIME_MS" \
    --gas-budget "$GAS_BUDGET"

echo "Done."

