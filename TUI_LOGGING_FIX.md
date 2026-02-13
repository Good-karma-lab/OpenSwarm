# TUI Logging Fix - Summary

## Problem
Log messages (INFO, WARN, DEBUG) were being printed to the console when TUI mode was enabled, which broke the TUI display. The log messages would overwrite the TUI interface, making it unreadable.

## Root Cause
The logging was configured to write to stdout/stderr regardless of whether TUI mode was enabled. Since the TUI also uses the terminal for rendering, the log messages would interfere with the TUI display.

## Solution
Modified the logging initialization in `main.rs` to detect TUI mode and redirect logs to a file instead of stdout/stderr when TUI is active.

### Changes Made (main.rs, lines 110-145)

```rust
// When TUI mode is enabled, redirect logs to a file
if cli.tui {
    // Create log directory and file
    let log_dir = std::env::temp_dir().join("openswarm-logs");
    std::fs::create_dir_all(&log_dir)?;
    let log_file = log_dir.join(format!("{}.log", config.agent.name));

    // Configure logger to write to file
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_ansi(false)  // No colors in log file
        .with_writer(std::sync::Mutex::new(file))
        .init();

    // Inform user where logs are being written
    eprintln!("📝 Logs are being written to: {}", log_file.display());
    eprintln!("   You can monitor logs with: tail -f {}", log_file.display());
} else {
    // Non-TUI mode: use normal stdout/stderr logging
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .init();
}
```

## Results

### Before Fix
- ❌ TUI display broken by log messages
- ❌ Log messages overwriting TUI panels
- ❌ Unreadable interface

### After Fix
- ✅ Clean TUI display with no log interference
- ✅ All logs redirected to file in TUI mode
- ✅ User informed of log file location
- ✅ Easy to monitor logs with `tail -f`

## Log File Location

When TUI mode is enabled, logs are written to:
```
/tmp/openswarm-logs/<agent-name>.log
```

On macOS, this is typically:
```
/var/folders/.../T/openswarm-logs/<agent-name>.log
```

The exact path is displayed when the connector starts.

## Usage

### Interactive TUI Mode
```bash
./run-node.sh -n "my-node"
```

- TUI displays cleanly without log interference
- Log file location is shown at startup
- Monitor logs separately: `tail -f /path/to/log/file.log`

### Non-TUI Mode
```bash
./run-node.sh -n "my-node" --no-tui
```

- Logs appear in console as before (normal behavior)
- No log file created

## Monitoring Logs in TUI Mode

When running in TUI mode, you can monitor logs in a separate terminal:

```bash
# Get the log file path from the startup message, then:
tail -f /var/folders/.../T/openswarm-logs/my-node.log
```

Or use a log viewer:
```bash
# Real-time colored logs
tail -f /path/to/log.log | grep --color=always "ERROR\|WARN\|INFO"

# Search logs
grep "some pattern" /path/to/log.log

# Last N lines
tail -100 /path/to/log.log
```

## Testing

Verification test results:
- ✅ Console output: 0 log messages (clean TUI)
- ✅ Log file: All log messages properly captured
- ✅ TUI display: No interference from logs
- ✅ User notification: Log file location shown

## Files Modified

- `crates/openswarm-connector/src/main.rs` - Lines 110-145 (logging initialization)

## Summary

The TUI logging issue has been completely fixed:
1. Logs no longer appear in the console when TUI mode is enabled
2. All logs are redirected to a file for clean TUI display
3. Users are informed where logs are being written
4. Non-TUI mode behavior unchanged (logs still go to console)
5. Easy log monitoring with `tail -f`

The TUI now displays cleanly without any log message interference! 🎉
