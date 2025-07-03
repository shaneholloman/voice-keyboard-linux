#!/bin/bash

# Voice Keyboard Runner Script
# This script runs the voice-keyboard with proper privilege handling

# Check if we're already running as root
if [ "$EUID" -eq 0 ]; then
    echo "Error: Don't run this script as root. It will handle privileges automatically."
    exit 1
fi

# Build the project first
echo "Building voice-keyboard..."
cargo build --release

if [ $? -ne 0 ]; then
    echo "Build failed!"
    exit 1
fi

# Run with sudo -E to preserve environment variables
echo "Starting voice-keyboard with privilege dropping..."
echo "Note: This will create a virtual keyboard as root, then drop privileges for audio access."
echo ""

sudo -E ./target/release/voice-keyboard "$@" 