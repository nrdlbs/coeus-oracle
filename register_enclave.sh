#!/bin/bash

# Check if both arguments are provided
if [ "$#" -lt 9 ] || [ "$#" -gt 10 ]; then
    echo "Usage: $0 <enclave_package_id> <app_package_id> <cap_object_id> <enclave_url> <module_name> <otw_name> <pcr0> <pcr1> <pcr2> [config_name]"
    echo "Example: $0 0x872852f77545c86a8bd9bdb8adc9e686b8573fc2a0dab0af44864bc1aecdaea9 0x2b70e34684d696a0a2847c793ee1e5b88a23289a7c04dd46249b95a9823367d9 0x86775ced1fdceae31d090cf48a11b4d8e4a613a2d49f657610c0bc287c8f0589 http://100.26.111.45:3000 weather Weather 911c87d0abc8c9840a0810d57dfb718865f35dc42010a2d5b30e7840b03edeea83a26aad51593ade1e47ab6cced4653e 911c87d0abc8c9840a0810d57dfb718865f35dc42010a2d5b30e7840b03edeea83a26aad51593ade1e47ab6cced4653e 21b9efbc184807662e966d34f390821309eeac6802309798826296bf3e8bec7c10edb30948c90ba67310f7b964fc500a"
    echo ""
    echo "Arguments:"
    echo "  enclave_package_id: Package ID of the enclave module"
    echo "  app_package_id: Package ID of the application module"
    echo "  cap_object_id: Object ID of the Cap<T> object"
    echo "  enclave_url: URL of the running enclave (e.g., http://100.26.111.45:3000)"
    echo "  module_name: Name of the application module"
    echo "  otw_name: Name of the One-Time Witness type"
    echo "  pcr0: PCR0 value (hex string, no 0x prefix)"
    echo "  pcr1: PCR1 value (hex string, no 0x prefix)"
    echo "  pcr2: PCR2 value (hex string, no 0x prefix)"
    echo ""
    echo "Optional arguments:"
    echo "  config_name: Name for the enclave config (default: 'enclave-config')"
    exit 1
fi

ENCLAVE_PACKAGE_ID=$1
APP_PACKAGE_ID=$2
CAP_OBJECT_ID=$3
ENCLAVE_URL=$4
MODULE_NAME=$5
OTW_NAME=$6
PCR0=$7
PCR1=$8
PCR2=$9
CONFIG_NAME=${10:-"enclave-config"}

echo "=== Step 1: Validating PCR values ==="
# Validate PCR values are not empty
if [ -z "$PCR0" ] || [ -z "$PCR1" ] || [ -z "$PCR2" ]; then
    echo "Error: PCR values cannot be empty"
    exit 1
fi

# Validate PCR values are valid hex strings (basic check)
if ! echo "$PCR0" | grep -qE '^[0-9a-fA-F]+$' || ! echo "$PCR1" | grep -qE '^[0-9a-fA-F]+$' || ! echo "$PCR2" | grep -qE '^[0-9a-fA-F]+$'; then
    echo "Error: PCR values must be valid hexadecimal strings (no 0x prefix)"
    exit 1
fi

# Validate PCR values have even length (hex pairs)
if [ $((${#PCR0} % 2)) -ne 0 ] || [ $((${#PCR1} % 2)) -ne 0 ] || [ $((${#PCR2} % 2)) -ne 0 ]; then
    echo "Error: PCR values must have even length (hex pairs)"
    exit 1
fi

echo "PCR0: $PCR0"
echo "PCR1: $PCR1"
echo "PCR2: $PCR2"

# Convert hex PCR values to vector format
PCR0_VECTOR=$(python3 - <<EOF
hex_string = "$PCR0"
byte_values = [str(int(hex_string[i:i+2], 16)) for i in range(0, len(hex_string), 2)]
rust_array = [f"{byte}u8" for byte in byte_values]
print(f"[{', '.join(rust_array)}]")
EOF
)

PCR1_VECTOR=$(python3 - <<EOF
hex_string = "$PCR1"
byte_values = [str(int(hex_string[i:i+2], 16)) for i in range(0, len(hex_string), 2)]
rust_array = [f"{byte}u8" for byte in byte_values]
print(f"[{', '.join(rust_array)}]")
EOF
)

PCR2_VECTOR=$(python3 - <<EOF
hex_string = "$PCR2"
byte_values = [str(int(hex_string[i:i+2], 16)) for i in range(0, len(hex_string), 2)]
rust_array = [f"{byte}u8" for byte in byte_values]
print(f"[{', '.join(rust_array)}]")
EOF
)

echo ""
echo "=== Step 2: Creating enclave config ==="
# Create enclave config and capture the transaction result
CONFIG_TX_OUTPUT=$(sui client ptb \
    --assign pcr0 "vector$PCR0_VECTOR" \
    --assign pcr1 "vector$PCR1_VECTOR" \
    --assign pcr2 "vector$PCR2_VECTOR" \
    --assign name "\"$CONFIG_NAME\"" \
    --move-call "${ENCLAVE_PACKAGE_ID}::enclave::create_enclave_config<${APP_PACKAGE_ID}::${MODULE_NAME}::${OTW_NAME}>" @${CAP_OBJECT_ID} name pcr0 pcr1 pcr2 \
    --gas-budget 100000000 \
    2>&1)

if [ $? -ne 0 ]; then
    echo "Error: Failed to create enclave config"
    echo "$CONFIG_TX_OUTPUT"
    exit 1
fi

# Extract transaction digest from output (works with both GNU and BSD grep)
CONFIG_TX_DIGEST=$(echo "$CONFIG_TX_OUTPUT" | grep "Transaction Digest:" | awk '{print $3}' | head -1)

if [ -z "$CONFIG_TX_DIGEST" ]; then
    # Try alternative: get from JSON output
    CONFIG_TX_JSON=$(sui client ptb \
        --assign pcr0 "vector$PCR0_VECTOR" \
        --assign pcr1 "vector$PCR1_VECTOR" \
        --assign pcr2 "vector$PCR2_VECTOR" \
        --assign name "\"$CONFIG_NAME\"" \
        --move-call "${ENCLAVE_PACKAGE_ID}::enclave::create_enclave_config<${APP_PACKAGE_ID}::${MODULE_NAME}::${OTW_NAME}>" @${CAP_OBJECT_ID} name pcr0 pcr1 pcr2 \
        --gas-budget 100000000 \
        --json 2>/dev/null)
    
    if [ $? -eq 0 ] && [ -n "$CONFIG_TX_JSON" ]; then
        CONFIG_TX_DIGEST=$(echo "$CONFIG_TX_JSON" | jq -r '.digest // empty')
    fi
    
    if [ -z "$CONFIG_TX_DIGEST" ]; then
        echo "Error: Could not extract transaction digest"
        echo "Transaction output:"
        echo "$CONFIG_TX_OUTPUT"
        exit 1
    fi
fi

echo "Transaction digest: $CONFIG_TX_DIGEST"

# Wait for transaction to be processed and extract the created config object ID
echo "Waiting for transaction to be processed..."
MAX_RETRIES=10
RETRY_COUNT=0
ENCLAVE_CONFIG_OBJECT_ID=""

while [ $RETRY_COUNT -lt $MAX_RETRIES ]; do
    sleep 2
    RETRY_COUNT=$((RETRY_COUNT + 1))
    
    # Try to get transaction result
    CONFIG_TX_RESULT=$(sui client tx-block "$CONFIG_TX_DIGEST" --json 2>/dev/null)
    
    if [ $? -eq 0 ] && [ -n "$CONFIG_TX_RESULT" ] && [ "$CONFIG_TX_RESULT" != "null" ]; then
        # Try different JSON paths to extract object ID
        # Path 1: objectChanges (top level - this is the correct path for tx-block)
        ENCLAVE_CONFIG_OBJECT_ID=$(echo "$CONFIG_TX_RESULT" | jq -r '.objectChanges[]? | select(.type == "created") | select(.objectType | contains("EnclaveConfig")) | .objectId' 2>/dev/null | head -1)
        
        # Path 2: effects.objectChanges
        if [ -z "$ENCLAVE_CONFIG_OBJECT_ID" ] || [ "$ENCLAVE_CONFIG_OBJECT_ID" == "null" ]; then
            ENCLAVE_CONFIG_OBJECT_ID=$(echo "$CONFIG_TX_RESULT" | jq -r '.effects.objectChanges[]? | select(.type == "created") | select(.objectType | contains("EnclaveConfig")) | .objectId' 2>/dev/null | head -1)
        fi
        
        # Path 3: effects.created (alternative structure)
        if [ -z "$ENCLAVE_CONFIG_OBJECT_ID" ] || [ "$ENCLAVE_CONFIG_OBJECT_ID" == "null" ]; then
            ENCLAVE_CONFIG_OBJECT_ID=$(echo "$CONFIG_TX_RESULT" | jq -r '.effects.created[]? | select(.owner | type == "Shared") | .reference.objectId' 2>/dev/null | head -1)
        fi
        
        # Path 4: transaction.effects (if nested differently)
        if [ -z "$ENCLAVE_CONFIG_OBJECT_ID" ] || [ "$ENCLAVE_CONFIG_OBJECT_ID" == "null" ]; then
            ENCLAVE_CONFIG_OBJECT_ID=$(echo "$CONFIG_TX_RESULT" | jq -r '.transaction.effects.objectChanges[]? | select(.type == "created") | select(.objectType | contains("EnclaveConfig")) | .objectId' 2>/dev/null | head -1)
        fi
        
        # Path 5: Look for any created object with EnclaveConfig in the type (recursive search)
        if [ -z "$ENCLAVE_CONFIG_OBJECT_ID" ] || [ "$ENCLAVE_CONFIG_OBJECT_ID" == "null" ]; then
            ENCLAVE_CONFIG_OBJECT_ID=$(echo "$CONFIG_TX_RESULT" | jq -r '.. | objects | select(.objectType? | contains("EnclaveConfig")) | .objectId' 2>/dev/null | head -1)
        fi
        
        if [ -n "$ENCLAVE_CONFIG_OBJECT_ID" ] && [ "$ENCLAVE_CONFIG_OBJECT_ID" != "null" ]; then
            break
        fi
    fi
    
    echo "Retry $RETRY_COUNT/$MAX_RETRIES: Transaction not ready yet, waiting..."
done

if [ -z "$ENCLAVE_CONFIG_OBJECT_ID" ] || [ "$ENCLAVE_CONFIG_OBJECT_ID" == "null" ]; then
    echo "Error: Could not extract enclave config object ID from transaction result"
    echo "Transaction digest: $CONFIG_TX_DIGEST"
    echo "Transaction result (first 1000 chars):"
    echo "$CONFIG_TX_RESULT" | head -c 1000
    echo ""
    echo "Full transaction result structure:"
    echo "$CONFIG_TX_RESULT" | jq '.' 2>/dev/null || echo "$CONFIG_TX_RESULT"
    echo ""
    echo "You can manually check the transaction with:"
    echo "  sui client tx-block $CONFIG_TX_DIGEST"
    exit 1
fi

echo "Created enclave config object ID: $ENCLAVE_CONFIG_OBJECT_ID"
export ENCLAVE_CONFIG_OBJECT_ID

echo ""
echo "=== Step 3: Fetching attestation ==="
# Fetch attestation and store the hex
ATTESTATION_HEX=$(curl -s $ENCLAVE_URL/get_attestation | jq -r '.attestation')

echo "got attestation, length=${#ATTESTATION_HEX}"

if [ ${#ATTESTATION_HEX} -eq 0 ]; then
    echo "Error: Attestation is empty. Please check status of $ENCLAVE_URL and its get_attestation endpoint."
    exit 1
fi

# Convert hex to array using Python
ATTESTATION_ARRAY=$(python3 - <<EOF
import sys

def hex_to_vector(hex_string):
    byte_values = [str(int(hex_string[i:i+2], 16)) for i in range(0, len(hex_string), 2)]
    rust_array = [f"{byte}u8" for byte in byte_values]
    return f"[{', '.join(rust_array)}]"

print(hex_to_vector("$ATTESTATION_HEX"))
EOF
)

echo ""
echo "=== Step 4: Registering enclave ==="
# Execute sui client command with the converted array and provided arguments
sui client ptb --assign v "vector$ATTESTATION_ARRAY" \
    --move-call "0x2::nitro_attestation::load_nitro_attestation" v @0x6 \
    --assign result \
    --move-call "${ENCLAVE_PACKAGE_ID}::enclave::register_enclave<${APP_PACKAGE_ID}::${MODULE_NAME}::${OTW_NAME}>" @${ENCLAVE_CONFIG_OBJECT_ID} result \
    --gas-budget 100000000

if [ $? -eq 0 ]; then
    echo ""
    echo "=== Success ==="
    echo "Enclave config object ID: $ENCLAVE_CONFIG_OBJECT_ID"
    echo "This ID has been saved to environment variable ENCLAVE_CONFIG_OBJECT_ID"
else
    echo "Error: Failed to register enclave"
    exit 1
fi