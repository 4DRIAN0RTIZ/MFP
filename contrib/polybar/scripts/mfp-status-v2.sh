#!/bin/bash
# MFP status for polybar - Stable version without flickering

PLAYER="org.mpris.MediaPlayer2.mfp"
STATE_FILE="/tmp/polybar-mfp-last-mpris"

# Function to check if MPRIS is available now
check_mpris_now() {
    # Quick check with timeout
    timeout 0.3 playerctl --player="$PLAYER" status >/dev/null 2>&1
}

# Function to check if MFP process is really running
check_mfp_process() {
    # Look for mfp processes, check they're not zombies
    local pids=$(pgrep -f "mfp" 2>/dev/null)
    
    for pid in $pids; do
        # Check process state (not zombie)
        local state=$(ps -o stat= -p "$pid" 2>/dev/null)
        if [ -n "$state" ] && [[ ! "$state" =~ Z ]]; then
            # Check command line contains "play" (mfp play)
            local cmdline=$(cat "/proc/$pid/cmdline" 2>/dev/null | tr '\0' ' ')
            if [[ "$cmdline" =~ mfp.*play ]] || [[ "$cmdline" =~ target.*debug.*mfp ]]; then
                return 0  # Found alive MFP process
            fi
        fi
    done
    
    return 1  # No alive MFP process found
}

# Update last MPRIS time if available now
if check_mpris_now; then
    date +%s > "$STATE_FILE"
fi

# Get current time
now=$(date +%s)

# Check MPRIS first (most authoritative)
if check_mpris_now; then
    # MPRIS is available - show playback info
    STATUS=$(timeout 0.5 playerctl --player="$PLAYER" status 2>/dev/null || echo "Unknown")
    TITLE=$(timeout 0.5 playerctl --player="$PLAYER" metadata xesam:title 2>/dev/null 2>/dev/null || echo "")
    
    case "$STATUS" in
        "Playing") ICON="" ;;
        "Paused")  ICON="" ;;
        "Stopped") ICON="" ;;
        *)         ICON="" ;;
    esac
    
    # Shorten title if needed
    if [ ${#TITLE} -gt 30 ]; then
        TITLE="${TITLE:0:27}..."
    fi
    
    if [ -n "$TITLE" ] && [ "$TITLE" != "" ]; then
        echo "$ICON $TITLE"
    else
        echo "$ICON"
    fi
    exit 0
fi

# MPRIS not available - check process
if check_mfp_process; then
    # Process is running but MPRIS not available
    # Check when we last had MPRIS
    if [ -f "$STATE_FILE" ]; then
        last_mpris=$(cat "$STATE_FILE")
        time_since_mpris=$((now - last_mpris))
        
        # If we lost MPRIS recently (< 4 seconds), show "Starting..."
        if [ $time_since_mpris -lt 4 ]; then
            echo " Starting..."
            exit 0
        fi
    fi
    
    # Process running but MPRIS gone for a while
    echo "▶ MFP"
    exit 0
fi

# No process running
# Clean up state file since process is gone
rm -f "$STATE_FILE" 2>/dev/null || true
echo "▶ MFP"