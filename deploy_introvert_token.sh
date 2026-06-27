#!/usr/bin/env bash
# ==============================================================================
# Introvert Sovereign Master Plan v2.0 - Token Deployment Automation
# ==============================================================================

set -euo pipefail

# IDENTITY UNITY: Derived from BIP-39 Master Seed
# Note: In production, the seed is never passed as a CLI arg.
# This script assumes 'solana-keygen' is configured to use the derived identity.

CLUSTER="devnet"
DECIMALS=9
INITIAL_SUPPLY=100000000

echo "🚀 Initiating Gasless Token Deployment on Solana $CLUSTER..."

# 1. Requirement Verification
for tool in solana spl-token; do
    if ! command -v "$tool" &> /dev/null; then
        echo "❌ Error: '$tool' not found."
        exit 1
    fi
done

# 2. Configuration
solana config set --url "$CLUSTER"

# 3. Identity Verification
# Authority derived from BIP-39 logic (Identity Unity)
AUTHORITY_PUBKEY=$(solana address)
echo "📍 Sovereign Authority: $AUTHORITY_PUBKEY"

# 4. Create Token Mint
echo "🪙  Creating SPL Token Mint..."
# Supporting 'Gasless Reward Claims' via Treasury model
MINT_OUTPUT=$(spl-token create-token --decimals "$DECIMALS")
INTROVERT_TOKEN_MINT=$(echo "$MINT_OUTPUT" | grep -oE "[a-zA-Z0-9]{32,44}" | head -n 1)

if [ -z "$INTROVERT_TOKEN_MINT" ]; then
    echo "❌ Error: Mint creation failed."
    exit 1
fi

echo "✅ Mint Created: $INTROVERT_TOKEN_MINT"

# 5. Treasury Vault Initialization (Associated Token Account)
echo "🏦 Initializing Treasury Vault (Gasless Co-signer Model)..."
spl-token create-account "$INTROVERT_TOKEN_MINT"

# 6. Minting Initial Supply
echo "🌱 Minting $INITIAL_SUPPLY INTR to Treasury..."
spl-token mint "$INTROVERT_TOKEN_MINT" "$INITIAL_SUPPLY"

echo "--------------------------------------------------------"
echo "🎉 SOVEREIGN TOKEN DEPLOYED 🎉"
echo "INTROVERT_TOKEN_MINT=\"$INTROVERT_TOKEN_MINT\""
echo "MODEL: Gasless Claims via Treasury Relay"
echo "--------------------------------------------------------"
