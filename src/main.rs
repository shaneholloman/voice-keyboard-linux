use anyhow::{Context, Result};
use clap::Parser;
use tracing::{error, info};

mod virtual_keyboard;
mod input_event;

use virtual_keyboard::VirtualKeyboard;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,
    
    /// Test mode - just create keyboard and exit
    #[arg(short, long)]
    test: bool,
    
    /// Device name for the virtual keyboard
    #[arg(short, long, default_value = "Voice Keyboard")]
    device_name: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    
    // Initialize logging
    let log_level = if args.verbose { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(format!("voice_keyboard={}", log_level))
        .init();
    
    info!("Starting Voice Keyboard v{}", env!("CARGO_PKG_VERSION"));
    
    // Check if we have access to /dev/uinput
    let uinput_path = std::path::Path::new("/dev/uinput");
    if !uinput_path.exists() {
        error!("/dev/uinput not found. Please load the uinput module:");
        error!("sudo modprobe uinput");
        return Err(anyhow::anyhow!("uinput module not loaded"));
    }

    // Check if we can access /dev/uinput
    match std::fs::OpenOptions::new().write(true).open(uinput_path) {
        Ok(_) => {
            info!("Successfully opened /dev/uinput");
        }
        Err(e) => {
            if !nix::unistd::geteuid().is_root() {
                error!("Cannot access /dev/uinput: {}", e);
                error!("Either:");
                error!("1. Run with root privileges: sudo {}", std::env::args().next().unwrap());
                error!("2. Add yourself to the 'input' group: sudo usermod -a -G input $USER");
                error!("   Then create udev rule: echo 'KERNEL==\"uinput\", GROUP=\"input\", MODE=\"0660\"' | sudo tee /etc/udev/rules.d/99-uinput.rules");
                error!("   Finally: sudo udevadm control --reload-rules && sudo udevadm trigger");
                error!("   (You'll need to log out and back in for group changes to take effect)");
                return Err(anyhow::anyhow!("Insufficient permissions to access /dev/uinput"));
            } else {
                error!("Failed to open /dev/uinput even with root privileges: {}", e);
                return Err(anyhow::anyhow!("Failed to open /dev/uinput"));
            }
        }
    }
    
    info!("Creating virtual keyboard: {}", args.device_name);
    
    // Create virtual keyboard
    let mut keyboard = VirtualKeyboard::new(&args.device_name)
        .context("Failed to create virtual keyboard")?;
    
    if args.test {
        info!("Test mode - demonstrating keyboard functionality");
        test_keyboard(&mut keyboard).await?;
        info!("Test completed successfully");
        return Ok(());
    }
    
    info!("Virtual keyboard created successfully");
    info!("The keyboard is now available to all applications");
    info!("Press Ctrl+C to exit");
    
    // Set up signal handling
    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .context("Failed to set up SIGTERM handler")?;
    let mut sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
        .context("Failed to set up SIGINT handler")?;
    
    // Keep the application running
    tokio::select! {
        _ = sigterm.recv() => {
            info!("Received SIGTERM, shutting down gracefully");
        }
        _ = sigint.recv() => {
            info!("Received SIGINT, shutting down gracefully");
        }
        _ = tokio::signal::ctrl_c() => {
            info!("Received Ctrl+C, shutting down gracefully");
        }
    }
    
    info!("Shutting down...");
    Ok(())
}

async fn test_keyboard(keyboard: &mut VirtualKeyboard) -> Result<()> {
    info!("Starting keyboard test in 3 seconds...");
    info!("Please focus on a text editor or terminal");
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
    
    // Test basic typing
    info!("Typing: 'Hello, World!'");
    keyboard.type_text("Hello, World!")?;
    
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    
    // Test Enter key
    info!("Pressing Enter");
    keyboard.press_enter()?;
    
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    
    // Test special characters
    info!("Typing special characters: !@#$%^&*()");
    keyboard.type_text("Special chars: !@#$%^&*()")?;
    
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    keyboard.press_enter()?;
    
    // Test backspace
    info!("Typing 'mistake' and then backspacing");
    keyboard.type_text("This is a mistake")?;
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    
    for _ in 0..7 { // Delete "mistake"
        keyboard.press_backspace()?;
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    }
    
    keyboard.type_text("correction!")?;
    keyboard.press_enter()?;
    
    info!("Keyboard test completed!");
    Ok(())
}
