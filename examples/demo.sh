#!/bin/bash
# Demo script for TestCtl protocol
# Tests TCP mode without requiring USB hardware

set -e

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}=== TestCtl Demo ===${NC}"
echo

# Build
echo -e "${BLUE}Building testctl...${NC}"
cargo build --release -p testctl --quiet
echo -e "${GREEN}✓ Build complete${NC}"
echo

# Start remote in background
echo -e "${BLUE}Starting remote server (TCP mode)...${NC}"
./target/release/testctl-remote --tcp --tcp-addr 127.0.0.1:9999 &
REMOTE_PID=$!
sleep 2
echo -e "${GREEN}✓ Remote running (PID: $REMOTE_PID)${NC}"
echo

# Cleanup function
cleanup() {
    echo
    echo -e "${BLUE}Cleaning up...${NC}"
    kill $REMOTE_PID 2>/dev/null || true
    wait $REMOTE_PID 2>/dev/null || true
    echo -e "${GREEN}✓ Done${NC}"
}
trap cleanup EXIT

# Test commands
MASTER="./target/release/testctl-master --tcp --remote-addr 127.0.0.1:9999"

echo -e "${BLUE}1. Ping remote${NC}"
$MASTER ping
echo

echo -e "${BLUE}2. Get device info${NC}"
$MASTER info
echo

echo -e "${BLUE}3. Get network status${NC}"
$MASTER network-status usb0
echo

echo -e "${BLUE}4. Start a test${NC}"
$MASTER test-start api_tests --test-case test_dns_status
echo

echo -e "${BLUE}5. Get test results${NC}"
$MASTER test-results test-1
echo

echo -e "${GREEN}=== All tests passed! ===${NC}"
echo
echo "Try interactive mode:"
echo "  $MASTER interactive"
