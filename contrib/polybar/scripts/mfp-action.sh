#!/bin/bash
# MFP action handler for polybar clicks

PLAYER="org.mpris.MediaPlayer2.mfp"

# Detect terminal
detect_terminal() {
    # Check environment variable first
    if [ -n "$TERMINAL" ]; then
        echo "$TERMINAL"
        return
    fi
    
    # Try common terminals
    for term in alacritty kitty termite xterm urxvt st foot; do
        if command -v "$term" >/dev/null 2>&1; then
            echo "$term"
            return
        fi
    done
    
    # Fallback to xterm (usually available)
    echo "xterm"
}

# Find MFP binary
find_mfp_binary() {
    # Check local development build first
    if [ -x "$HOME/MFP/target/debug/mfp" ]; then
        echo "$HOME/MFP/target/debug/mfp"
        return
    fi
    
    # Check global installation
    if command -v mfp >/dev/null 2>&1; then
        command -v mfp
        return
    fi
    
    # Check cargo bin
    if [ -x "$HOME/.cargo/bin/mfp" ]; then
        echo "$HOME/.cargo/bin/mfp"
        return
    fi
    
    # Not found
    echo ""
}

MFP_BIN=$(find_mfp_binary)
MFP_PROCESS="mfp play"  # Generic process search
TERMINAL=$(detect_terminal)

# Function to check if MFP is running
is_mfp_running() {
    pgrep -f "$MFP_PROCESS" >/dev/null 2>&1
}

# Function to check if MPRIS player is available
is_mpris_available() {
    playerctl --player="$PLAYER" status >/dev/null 2>&1
}

# Function to start MFP
start_mfp() {
    if [ -n "$MFP_BIN" ] && [ -x "$MFP_BIN" ]; then
        # Launch existing binary in terminal
        $TERMINAL -e "$MFP_BIN" play &
    else
        # Try cargo run if binary not built
        $TERMINAL -e "cd $HOME/MFP && cargo run -- play" &
    fi
    sleep 2  # Give time for MPRIS to register
}

# Determine action based on argument (button)
BUTTON="$1"

case "$BUTTON" in
    "left")
        # Left click: play/pause or start MFP
        if is_mpris_available; then
            playerctl --player="$PLAYER" play-pause
        elif is_mfp_running; then
            # MFP is running but MPRIS not ready yet, wait and try
            sleep 1
            if is_mpris_available; then
                playerctl --player="$PLAYER" play-pause
            fi
        else
            # Start MFP from scratch
            start_mfp
        fi
        ;;
    "right")
        # Right click: context menu (using rofi if available)
        if command -v rofi >/dev/null 2>&1; then
            if is_mpris_available; then
                # MPRIS is available, show media controls menu
                STATUS=$(playerctl --player="$PLAYER" status 2>/dev/null)
                if [ "$STATUS" = "Playing" ]; then
                    PLAY_PAUSE="⏸️ Pause"
                else
                    PLAY_PAUSE="▶️ Play"
                fi
                
                CHOICE=$(echo -e "$PLAY_PAUSE\n⏭️ Next\n⏮️ Previous\n⏹️ Stop\n✖️ Quit MFP" | \
                    rofi -dmenu -p "MFP Controls" -theme-str 'window {width: 20%;}' 2>/dev/null)
                
                case "$CHOICE" in
                    *"Play"|*"Pause")
                        playerctl --player="$PLAYER" play-pause
                        ;;
                    *"Next")
                        playerctl --player="$PLAYER" next
                        ;;
                    *"Previous")
                        playerctl --player="$PLAYER" previous
                        ;;
                    *"Stop")
                        playerctl --player="$PLAYER" stop
                        ;;
                    *"Quit MFP")
                        pkill -f "$MFP_PROCESS"
                        ;;
                esac
            elif is_mfp_running; then
                # MFP running but no MPRIS yet
                echo "MFP is starting..." | rofi -dmenu -p "MFP" 2>/dev/null
            else
                # Start MFP option
                CHOICE=$(echo -e "▶️ Start MFP" | \
                    rofi -dmenu -p "MFP" -theme-str 'window {width: 15%;}' 2>/dev/null)
                if [[ "$CHOICE" == *"Start"* ]]; then
                    start_mfp
                fi
            fi
        else
            # No rofi, just start/stop
            if is_mfp_running; then
                pkill -f "$MFP_PROCESS"
            else
                start_mfp
            fi
        fi
        ;;
    "middle")
        # Middle click: next track or start MFP with shuffle
        if is_mpris_available; then
            playerctl --player="$PLAYER" next
        elif is_mfp_running; then
            # Can't send next to non-MPRIS MFP
            echo "MPRIS not ready yet" >&2
        else
            # Start MFP with shuffle
            $TERMINAL -e "cd $HOME/MFP && cargo run -- play --shuffle" &
        fi
        ;;
    "scroll-up")
        # Scroll up: volume up
        if is_mpris_available; then
            playerctl --player="$PLAYER" volume 0.05+
        fi
        ;;
    "scroll-down")
        # Scroll down: volume down
        if is_mpris_available; then
            playerctl --player="$PLAYER" volume 0.05-
        fi
        ;;
    *)
        # Unknown button, default to left click action
        if is_mpris_available; then
            playerctl --player="$PLAYER" play-pause
        elif is_mfp_running; then
            sleep 1
            if is_mpris_available; then
                playerctl --player="$PLAYER" play-pause
            fi
        else
            start_mfp
        fi
        ;;
esac