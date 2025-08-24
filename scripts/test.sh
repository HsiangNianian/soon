#!/bin/bash
# Simple test script for soon CLI tool

set -e

echo "🧪 Running Soon CLI Tests..."

# Test 1: Build succeeds
echo "1. Testing build..."
cargo build --quiet

# Test 2: Unit tests pass
echo "2. Running unit tests..."
cargo test --quiet

# Test 3: Basic CLI functionality
echo "3. Testing CLI help..."
./target/debug/soon --help > /dev/null

echo "4. Testing version command..."
./target/debug/soon version > /dev/null

echo "5. Testing which command..."
./target/debug/soon which > /dev/null

# Test 4: Release build
echo "6. Testing release build..."
cargo build --release --quiet

echo "✅ All tests passed!"