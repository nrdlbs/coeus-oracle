#!/bin/bash

# Script to call oracle::feed::new using sui client ptb
# Usage: ./call_new_feed.sh <extension> <return_type> <object_id> <allow_update_timestamp_ms>
# Example: ./call_new_feed.sh python string 0x1234567890abcdef1234567890abcdef12345678 1000000

set -e

# Package ID from the published transaction
PACKAGE_ID="0x1432eec6bbbaa52dbebac2e7678d52ad03e551e5427aa6120f87ec3d8223b71d"

# Check if required arguments are provided
if [ $# -lt 4 ]; then
    echo "Usage: $0 <extension> <return_type> <object_id> <allow_update_timestamp_ms>"
    echo "  extension: python"
    echo "  return_type: string | boolean | number | vector | empty"
    echo "  object_id: Hex string ID (e.g., 0x1234...)"
    echo "  allow_update_timestamp_ms: u64 timestamp in milliseconds"
    echo ""
    echo "Example: $0 python string 0x1234567890abcdef1234567890abcdef12345678 1000000"
    exit 1
fi

EXTENSION_TYPE=$1
RETURN_TYPE=$2
OBJECT_ID=$3
ALLOW_UPDATE_TIMESTAMP_MS=$4

# Validate extension type
if [[ ! "$EXTENSION_TYPE" =~ ^(python)$ ]]; then
    echo "Error: extension must be one of: python"
    exit 1
fi

# Validate return type
if [[ ! "$RETURN_TYPE" =~ ^(string|boolean|number|vector|empty)$ ]]; then
    echo "Error: return_type must be one of: string, boolean, number, vector, empty"
    exit 1
fi

# Convert extension string to individual byte values for vector<u8>
# We need to create a vector of u8 values from the string bytes
EXTENSION_BYTES_ARRAY=()
for (( i=0; i<${#EXTENSION_TYPE}; i++ )); do
    char="${EXTENSION_TYPE:$i:1}"
    byte_value=$(printf "%d" "'$char")
    EXTENSION_BYTES_ARRAY+=("$byte_value")
done

# Convert return_type string to individual byte values for vector<u8>
RETURN_TYPE_BYTES_ARRAY=()
for (( i=0; i<${#RETURN_TYPE}; i++ )); do
    char="${RETURN_TYPE:$i:1}"
    byte_value=$(printf "%d" "'$char")
    RETURN_TYPE_BYTES_ARRAY+=("$byte_value")
done

# Join array elements with commas and wrap in brackets for PTB
EXTENSION_BYTES_STR=$(IFS=','; echo "[${EXTENSION_BYTES_ARRAY[*]}]")
RETURN_TYPE_BYTES_STR=$(IFS=','; echo "[${RETURN_TYPE_BYTES_ARRAY[*]}]")

echo "Calling construct_code_extension with extension: $EXTENSION_TYPE"
echo "Extension bytes: $EXTENSION_BYTES_STR"
echo "Calling construct_return_type with return_type: $RETURN_TYPE"
echo "Return type bytes: $RETURN_TYPE_BYTES_STR"
echo "Object ID: $OBJECT_ID"
echo "Allow update timestamp (ms): $ALLOW_UPDATE_TIMESTAMP_MS"
echo ""

# Build the PTB command
# Step 1: Create vector<u8> from extension string bytes
# Step 2: Call construct_code_extension with the vector and assign to variable
# Step 3: Create vector<u8> from return_type string bytes
# Step 4: Call construct_return_type with the vector and assign to variable
# Step 5: Create ID from address/hex string if needed
# Step 6: Call new with object_id, extension, return_type, and timestamp parameters

# Format object ID for PTB (use @ prefix for object IDs)
if [[ "$OBJECT_ID" =~ ^0x ]]; then
    OBJECT_ID_ARG="@${OBJECT_ID}"
else
    OBJECT_ID_ARG="@0x${OBJECT_ID}"
fi

sui client ptb \
    --make-move-vec "<u8>" "${EXTENSION_BYTES_STR}" \
    --assign extension_vec \
    --move-call "${PACKAGE_ID}::feed::construct_code_extension" extension_vec \
    --assign code_ext \
    --make-move-vec "<u8>" "${RETURN_TYPE_BYTES_STR}" \
    --assign return_type_vec \
    --move-call "${PACKAGE_ID}::feed::construct_return_type" return_type_vec \
    --assign ret_type \
    --move-call "${PACKAGE_ID}::feed::new" "${OBJECT_ID_ARG} code_ext ret_type ${ALLOW_UPDATE_TIMESTAMP_MS}" \
    --summary

echo ""
echo "Transaction completed successfully!"

