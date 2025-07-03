# Voice Keyboard

A virtual keyboard application for Linux that creates a virtual input device using the uinput kernel module. This application is designed to be the foundation for a speech-to-text virtual keyboard system.

## Features

- Creates a virtual keyboard device that appears as a standard input device to all applications
- Supports typing text, special characters, and key combinations
- Built with Rust for performance and memory safety
- Includes a test mode to verify functionality
- Graceful signal handling for clean shutdown

## Prerequisites

- Linux system with uinput kernel module
- Root privileges (required for creating virtual input devices)
- Rust toolchain (for building from source)

## Installation

1. Clone or download the source code
2. Build the project:
   ```bash
   cargo build --release
   ```

## Usage

### Load the uinput module

Before running the application, ensure the uinput kernel module is loaded:

```bash
sudo modprobe uinput
```

### Run the application

The application requires root privileges to create virtual input devices:

```bash
sudo ./target/release/voice-keyboard
```

### Command-line options

- `-v, --verbose`: Enable verbose logging
- `-t, --test`: Run in test mode (demonstrates keyboard functionality)
- `-d, --device-name <NAME>`: Set custom device name (default: "Voice Keyboard")
- `-h, --help`: Show help information

### Test mode

To verify the keyboard is working correctly, run in test mode:

```bash
sudo ./target/release/voice-keyboard --test
```

This will:
1. Create the virtual keyboard
2. Wait 3 seconds (focus on a text editor or terminal)
3. Type "Hello, World!" followed by Enter
4. Type special characters
5. Demonstrate backspace functionality
6. Exit automatically

## Architecture

The application consists of three main modules:

- **main.rs**: Application entry point, command-line parsing, and signal handling
- **virtual_keyboard.rs**: Virtual keyboard implementation using uinput
- **input_event.rs**: Linux input event structures and key code definitions

## Key Components

### VirtualKeyboard
The main struct that manages the virtual keyboard device:
- Creates and configures the uinput device
- Sends key events to the system
- Provides high-level typing methods

### Input Events
Linux input events are used to communicate with the kernel:
- Key press/release events
- Synchronization events
- Character-to-keycode mapping

## Future Enhancements

This skeleton application is designed to be extended with:
- Speech recognition integration
- Audio capture and processing
- Real-time speech-to-text conversion
- Configuration management
- GUI interface

## Security Considerations

- The application requires root privileges to access `/dev/uinput`
- Virtual keyboard events are sent to the currently focused application
- All typed content is visible to applications that can read input events

## Troubleshooting

### Permission Denied
- Ensure you're running with `sudo`
- Check that `/dev/uinput` exists and is accessible

### uinput module not found
- Load the module: `sudo modprobe uinput`
- Add to `/etc/modules` for automatic loading on boot

### Device not appearing
- Check system logs: `journalctl -f`
- Verify the device was created: `ls /dev/input/`
- Use `evtest` to test the device functionality

## License

This project is released under the MIT License. 