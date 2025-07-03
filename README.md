# Voice Keyboard

A Rust-based voice-controlled keyboard that converts speech to text and types it into any application.

## Features

- **Voice-to-Text**: Real-time speech recognition using WebSocket STT services
- **Virtual Keyboard**: Creates a virtual input device that works with all applications
- **Privilege Dropping**: Secure privilege management - starts as root to create virtual keyboard, then drops to user privileges for audio access
- **Cross-Platform Audio**: Works with PipeWire, PulseAudio, and ALSA
- **Real-time Processing**: Low-latency audio streaming and transcription

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
cargo build --release
```

## Usage

### Easy Method (Recommended)

Use the provided runner script:

```bash
# Test audio input
./run.sh --test-audio

# Test speech-to-text (requires STT service)
./run.sh --test-stt

# Debug speech-to-text (print transcripts without typing)
./run.sh --debug-stt

# Use custom STT service URL
./run.sh --test-stt --stt-url "wss://your-stt-service.com/"
```

### Manual Method

```bash
# Build and run with proper privilege handling
cargo build --release
sudo -E ./target/release/voice-keyboard --test-stt
```

**Important**: Always use `sudo -E` to preserve environment variables needed for audio access.

## Speech-to-Text Service

The application connects to a WebSocket-based STT service. The default URL is `ws://localhost:8765`.

### Expected STT Service API

The service should accept:
- WebSocket connections with query parameters: `sample_rate`, `preflight_threshold`, `eot_threshold`, `eot_timeout_ms`
- Binary audio data (16-bit PCM, little-endian)
- Return JSON transcription results with `event`, `transcript`, and confidence fields

### Example STT Services

- Local: Run your own STT service on `localhost:8765`
- Remote: Use `--stt-url` to connect to external services
- Compatible with Deepgram River API format

## Command Line Options

```bash
voice-keyboard [OPTIONS]

OPTIONS:
    --test-audio         Test audio input and show levels
    --test-stt          Test speech-to-text functionality
    --debug-stt         Debug speech-to-text (print transcripts without typing)
    --stt-url <URL>     Custom STT service URL (default: ws://localhost:8765)
    -h, --help          Print help information
    -V, --version       Print version information
```

## How It Works

1. **Initialization**: Application starts with root privileges
2. **Virtual Keyboard**: Creates `/dev/uinput` device as root
3. **Privilege Drop**: Drops to original user privileges
4. **Audio Access**: Accesses PipeWire/PulseAudio in user space
5. **Speech Recognition**: Streams audio to STT service
6. **Text Input**: Types transcribed text via virtual keyboard

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

### Build Issues

If you get compilation errors:

1. **Update Rust**: `rustup update`
2. **Install dependencies**: See Prerequisites section
3. **Clean build**: `cargo clean && cargo build`

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
- **VirtualKeyboard**: Manages uinput device lifecycle
- **AudioInput**: Cross-platform audio capture
- **SttClient**: WebSocket-based speech-to-text client

## License

[Add your license here]

## Contributing

[Add contribution guidelines here] 