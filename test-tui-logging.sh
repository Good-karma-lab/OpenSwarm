#!/bin/bash

set -e

echo "================================"
echo "Testing TUI Logging Fix"
echo "================================"
echo ""

# Clean up
pkill -f "log-test" 2>/dev/null || true
rm -f /tmp/tui-log-test.out
sleep 1

echo "Test: Logs should go to file in TUI mode, not break display"
echo "-----------------------------------------------------------"

# Start connector in TUI mode (in background for testing)
(./run-node.sh -n "log-test" > /tmp/tui-log-test.out 2>&1) &
TEST_PID=$!

# Wait for startup
sleep 5

echo ""
echo "Checking console output (should not contain INFO/DEBUG logs):"
echo "-----------------------------------------------------------"
if grep -E "INFO|DEBUG|WARN.*openswarm" /tmp/tui-log-test.out | head -5; then
    echo ""
    echo "❌ FAILED: Log messages found in console output (breaks TUI)"
    kill $TEST_PID 2>/dev/null || true
    exit 1
else
    echo "✅ PASSED: No log messages in console output"
fi

echo ""
echo "Checking if logs were redirected to file:"
echo "-----------------------------------------------------------"
LOG_FILE=$(ls -t /tmp/openswarm-logs/log-test.log 2>/dev/null | head -1)
if [ -n "$LOG_FILE" ] && [ -f "$LOG_FILE" ]; then
    echo "✅ PASSED: Log file created at: $LOG_FILE"
    echo ""
    echo "Sample log entries:"
    tail -5 "$LOG_FILE"
else
    echo "❌ FAILED: Log file not found"
    kill $TEST_PID 2>/dev/null || true
    exit 1
fi

# Check if log file location message was printed
if grep -q "Logs are being written to" /tmp/tui-log-test.out; then
    echo ""
    echo "✅ PASSED: Log file location message shown to user"
else
    echo ""
    echo "⚠️  WARNING: Log file location not shown (users might not know where logs are)"
fi

# Cleanup
kill $TEST_PID 2>/dev/null || true
pkill -f "log-test" 2>/dev/null || true

echo ""
echo "================================"
echo "Summary"
echo "================================"
echo "✅ TUI logging fix works correctly"
echo "✅ Logs redirected to file instead of console"
echo "✅ TUI display will not be broken by log messages"
echo ""
