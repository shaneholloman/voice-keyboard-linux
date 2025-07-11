# Voice Keyboard

A voice-controlled Linux virtual keyboard that converts speech to text and types it into any application.

As a result of directly targeting Linux as a driver, this works with all Linux applications.

## Features

- **Voice-to-Text**: Real-time speech recognition using WebSocket STT services
- **Virtual Keyboard**: Creates a virtual input device that works with all applications
- **Incremental Typing**: Smart transcript updates with minimal backspacing for real-time corrections

## Architecture

The application solves a common Linux privilege problem:
- **Virtual keyboard creation** requires root access to `/dev/uinput`
- **Audio input** requires user-space access to PipeWire/PulseAudio

**Solution**: The application starts with root privileges, creates the virtual keyboard, then drops privileges to access the user's audio session.

## Installation

### Prerequisites

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install required system packages (Fedora/RHEL)
sudo dnf install alsa-lib-devel

# Install required system packages (Ubuntu/Debian)
sudo apt install libasound2-dev
```

### Build

```bash
git clone <repository-url>
cd voice-keyboard
cargo build
```

## Usage

### Easy Method (Recommended)

Use the provided runner script:

```bash
./run.sh
```

### Manual Method

```bash
# Build and run with proper privilege handling
cargo build
sudo -E ./target/debug/voice-keyboard --test-stt
```

**Important**: Always use `sudo -E` to preserve environment variables needed for audio access.

## Speech-to-Text Service

The application connects to the Deepgram "River" STT service. The default URL is `wss://river.sandbox.deepgram.com`.

## Command Line Options

```bash
voice-keyboard [OPTIONS]

OPTIONS:
    --test-audio        Test audio input and show levels
    --test-stt          Test speech-to-text functionality (default if no other mode specified)
    --debug-stt         Debug speech-to-text (print transcripts without typing)
    --stt-url <URL>     Custom STT service URL (default: wss://river.sandbox.deepgram.com)
    -h, --help          Print help information
    -V, --version       Print version information
```

**Note**: If no mode is specified, the application defaults to `--test-stt` behavior.

## How It Works

1. **Initialization**: Application starts with root privileges
2. **Virtual Keyboard**: Creates `/dev/uinput` device as root
3. **Privilege Drop**: Drops to original user privileges
4. **Audio Access**: Accesses PipeWire/PulseAudio in user space
5. **Speech Recognition**: Streams audio to STT service
6. **Incremental Typing**: Updates text in real-time with smart backspacing
7. **Turn Finalization**: Clears tracking on "EndOfTurn" events (user presses Enter manually)

### Transcript Handling

The application provides sophisticated real-time transcript updates:

- **Incremental Updates**: As speech is recognized, the application updates the typed text by finding the common prefix between the current and new transcript, backspacing only the changed portion, and typing the new ending
- **Smart Backspacing**: Minimizes cursor movement by only removing characters that actually changed
- **Turn Management**: On "EndOfTurn" events, the application clears its internal tracking but doesn't automatically press Enter, allowing users to review before submitting

## Security

- **Minimal Root Time**: Only root during virtual keyboard creation
- **Environment Preservation**: Maintains user's audio session access
- **Clean Privilege Drop**: Properly drops both user and group privileges
- **No System Changes**: No permanent system configuration required

## Troubleshooting

### Audio Issues

If you get "Host is down" or "I/O error" when testing audio:

1. **Use `sudo -E`**: Always preserve environment variables
2. **Check PipeWire**: Ensure PipeWire is running: `systemctl --user status pipewire`
3. **Test without sudo**: Try `./target/debug/voice-keyboard --test-audio` (will fail on keyboard creation but audio should work)

### Permission Issues

If you get "Permission denied" for `/dev/uinput`:

1. **Check uinput module**: `sudo modprobe uinput`
2. **Verify device exists**: `ls -la /dev/uinput`
3. **Use sudo**: The application is designed to run with `sudo -E`

## Development

### Project Structure

```
src/
├── main.rs              # Main application and privilege dropping
├── virtual_keyboard.rs  # Virtual keyboard device management
├── audio_input.rs       # Audio capture and processing
├── stt_client.rs        # WebSocket STT client
└── input_event.rs       # Linux input event constants
```

### Key Components

- **OriginalUser**: Captures and restores user context
- **VirtualKeyboard**: Manages uinput device lifecycle with smart transcript updates
- **AudioInput**: Cross-platform audio capture
- **SttClient**: WebSocket-based speech-to-text client
- **AudioBuffer**: Manages audio chunking for STT streaming

## License

[Add your license here]

