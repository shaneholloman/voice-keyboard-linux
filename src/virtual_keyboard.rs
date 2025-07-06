#![allow(dead_code)]

use anyhow::{Context, Result};
use nix::fcntl::{open, OFlag};
use nix::sys::stat::Mode;
use nix::unistd::close;
use std::io::Write;
use std::os::unix::io::FromRawFd;
use tracing::{debug, error, info, warn};

use crate::input_event::*;

// Define ioctl macros for uinput
// The nix ioctl_write_int! macro requires the ioctl type and number
nix::ioctl_write_int!(ui_set_evbit, b'U', 100);
nix::ioctl_write_int!(ui_set_keybit, b'U', 101);
nix::ioctl_none!(ui_dev_create, b'U', 1);
nix::ioctl_none!(ui_dev_destroy, b'U', 2);

pub struct VirtualKeyboard {
    fd: i32,
    name: String,
    current_text: String,
}

impl VirtualKeyboard {
    pub fn new(device_name: &str) -> Result<Self> {
        info!("Creating virtual keyboard device: {}", device_name);

        // Open uinput device
        let fd = open(
            "/dev/uinput",
            OFlag::O_WRONLY | OFlag::O_NONBLOCK,
            Mode::empty(),
        )
        .context("Failed to open /dev/uinput")?;

        debug!("Opened uinput device with fd: {}", fd);

        // Enable key events
        unsafe {
            ui_set_evbit(fd, EV_KEY as u64).context("Failed to enable key events")?;
        }

        // Enable all required key codes
        let keycodes = get_all_keycodes();
        debug!("Enabling {} key codes", keycodes.len());

        for keycode in keycodes {
            unsafe {
                ui_set_keybit(fd, keycode as u64)
                    .context(format!("Failed to enable key code {}", keycode))?;
            }
        }

        // Set up device using the legacy uinput_user_dev interface
        let mut uidev = crate::input_event::UInputUserDev::default();
        // Set device name
        let name_bytes = device_name.as_bytes();
        let copy_len = std::cmp::min(name_bytes.len(), uidev.name.len() - 1);
        uidev.name[..copy_len].copy_from_slice(&name_bytes[..copy_len]);
        // Set device id
        uidev.id.bustype = 0x03; // USB
        uidev.id.vendor = 0x1234;
        uidev.id.product = 0x5678;
        uidev.id.version = 1;
        uidev.ff_effects_max = 0;
        // Write the uinput_user_dev structure
        let uidev_bytes = unsafe {
            std::slice::from_raw_parts(
                &uidev as *const crate::input_event::UInputUserDev as *const u8,
                std::mem::size_of::<crate::input_event::UInputUserDev>(),
            )
        };
        let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
        file.write_all(uidev_bytes)
            .context("Failed to write device setup")?;
        // Prevent the file from being closed when it goes out of scope
        std::mem::forget(file);
        // Create the device
        unsafe {
            ui_dev_create(fd).context("Failed to create uinput device")?;
        }
        info!("Virtual keyboard '{}' created successfully", device_name);
        Ok(Self {
            fd,
            name: device_name.to_string(),
            current_text: String::new(),
        })
    }

    fn send_event(&self, event: InputEvent) -> Result<()> {
        let event_bytes = unsafe {
            std::slice::from_raw_parts(
                &event as *const InputEvent as *const u8,
                std::mem::size_of::<InputEvent>(),
            )
        };

        let bytes_written = unsafe {
            libc::write(
                self.fd,
                event_bytes.as_ptr() as *const libc::c_void,
                event_bytes.len(),
            )
        };

        if bytes_written == -1 {
            let errno = std::io::Error::last_os_error();
            return Err(anyhow::anyhow!("Failed to write event: {}", errno));
        }

        if bytes_written != event_bytes.len() as isize {
            return Err(anyhow::anyhow!(
                "Partial write: expected {} bytes, wrote {}",
                event_bytes.len(),
                bytes_written
            ));
        }

        Ok(())
    }

    fn send_key(&self, keycode: u16, pressed: bool) -> Result<()> {
        debug!("Sending key: {} (pressed: {})", keycode, pressed);

        // Send key event
        let key_event = InputEvent::key_event(keycode, pressed);
        self.send_event(key_event)?;

        // Send synchronization event
        let syn_event = InputEvent::syn_event();
        self.send_event(syn_event)?;

        Ok(())
    }

    pub fn press_key(&self, keycode: u16) -> Result<()> {
        self.send_key(keycode, true)?;
        self.send_key(keycode, false)?;
        Ok(())
    }

    pub fn type_text(&self, text: &str) -> Result<()> {
        debug!("Typing text: '{}'", text);

        for c in text.chars() {
            if let Some((keycode, needs_shift)) = char_to_keycode(c) {
                if needs_shift {
                    // Press shift
                    self.send_key(KEY_LEFTSHIFT, true)?;

                    // Press the key
                    self.send_key(keycode, true)?;
                    self.send_key(keycode, false)?;

                    // Release shift
                    self.send_key(KEY_LEFTSHIFT, false)?;
                } else {
                    // Just press the key
                    self.press_key(keycode)?;
                }

                // Small delay between characters for more natural typing
                std::thread::sleep(std::time::Duration::from_millis(10));
            } else {
                warn!("Unsupported character: '{}'", c);
            }
        }

        Ok(())
    }

    pub fn press_enter(&self) -> Result<()> {
        self.press_key(KEY_ENTER)
    }

    pub fn press_backspace(&self) -> Result<()> {
        self.press_key(KEY_BACKSPACE)
    }

    pub fn press_tab(&self) -> Result<()> {
        self.press_key(KEY_TAB)
    }

    pub fn press_escape(&self) -> Result<()> {
        self.press_key(KEY_ESC)
    }

    pub fn press_space(&self) -> Result<()> {
        self.press_key(KEY_SPACE)
    }

    // Special key combinations
    pub fn press_ctrl_c(&self) -> Result<()> {
        self.send_key(KEY_LEFTCTRL, true)?;
        self.send_key(KEY_C, true)?;
        self.send_key(KEY_C, false)?;
        self.send_key(KEY_LEFTCTRL, false)?;
        Ok(())
    }

    pub fn press_ctrl_v(&self) -> Result<()> {
        self.send_key(KEY_LEFTCTRL, true)?;
        self.send_key(KEY_V, true)?;
        self.send_key(KEY_V, false)?;
        self.send_key(KEY_LEFTCTRL, false)?;
        Ok(())
    }

    pub fn press_alt_tab(&self) -> Result<()> {
        self.send_key(KEY_LEFTALT, true)?;
        self.send_key(KEY_TAB, true)?;
        self.send_key(KEY_TAB, false)?;
        self.send_key(KEY_LEFTALT, false)?;
        Ok(())
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }

    /// Handle incremental transcript updates
    /// This method will:
    /// 1. Type new characters if the new transcript extends the current one
    /// 2. Only backspace the characters that actually changed, then type the new ending
    pub fn update_transcript(&mut self, new_transcript: &str) -> Result<()> {
        debug!("Updating transcript from '{}' to '{}'", self.current_text, new_transcript);

        // If the new transcript is empty, clear everything
        if new_transcript.is_empty() {
            self.clear_current_text()?;
            return Ok(());
        }

        // Check if the new transcript is just an extension of the current one
        if new_transcript.starts_with(&self.current_text) {
            // Just type the new characters
            let new_chars = &new_transcript[self.current_text.len()..];
            if !new_chars.is_empty() {
                debug!("Typing new characters: '{}'", new_chars);
                self.type_text(new_chars)?;
                self.current_text = new_transcript.to_string();
            }
        } else {
            // Find the common prefix between current and new transcript
            let common_prefix_len = self.current_text
                .chars()
                .zip(new_transcript.chars())
                .take_while(|(a, b)| a == b)
                .count();
            
            let current_chars: Vec<char> = self.current_text.chars().collect();
            let chars_to_backspace = current_chars.len() - common_prefix_len;
            
            debug!("Common prefix length: {}, need to backspace {} characters", 
                   common_prefix_len, chars_to_backspace);
            
            // Only backspace the characters that differ
            if chars_to_backspace > 0 {
                debug!("Backspacing {} characters", chars_to_backspace);
                for _ in 0..chars_to_backspace {
                    self.press_backspace()?;
                }
            }
            
            // Type the new ending (everything after the common prefix)
            let new_chars: Vec<char> = new_transcript.chars().collect();
            if common_prefix_len < new_chars.len() {
                let new_ending: String = new_chars[common_prefix_len..].iter().collect();
                debug!("Typing new ending: '{}'", new_ending);
                self.type_text(&new_ending)?;
            }
            
            self.current_text = new_transcript.to_string();
        }

        Ok(())
    }

    /// Finalize the current transcript (without pressing enter)
    /// The user can review the text and press enter manually when ready
    pub fn finalize_transcript(&mut self) -> Result<()> {
        debug!("Finalizing transcript: '{}'", self.current_text);
        
        // Just clear the current text tracking, but don't press enter
        // The user can press enter manually when they're ready
        self.current_text.clear();
        
        Ok(())
    }

    /// Clear the current text by backspacing
    fn clear_current_text(&mut self) -> Result<()> {
        if !self.current_text.is_empty() {
            self.backspace_current_text()?;
        }
        Ok(())
    }

    /// Backspace all characters in the current text
    fn backspace_current_text(&mut self) -> Result<()> {
        let char_count = self.current_text.chars().count();
        debug!("Backspacing {} characters", char_count);
        
        for _ in 0..char_count {
            self.press_backspace()?;
            // Small delay between backspaces for reliability
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        
        self.current_text.clear();
        Ok(())
    }

    /// Get the current text that has been typed
    pub fn get_current_text(&self) -> &str {
        &self.current_text
    }
}

impl Drop for VirtualKeyboard {
    fn drop(&mut self) {
        info!("Destroying virtual keyboard '{}'", self.name);

        // Destroy the device
        unsafe {
            if let Err(e) = ui_dev_destroy(self.fd) {
                error!("Failed to destroy uinput device: {}", e);
            }
        }

        // Close the file descriptor
        if let Err(e) = close(self.fd) {
            error!("Failed to close uinput fd: {}", e);
        }

        debug!("Virtual keyboard '{}' destroyed", self.name);
    }
}

// Safety: VirtualKeyboard only contains a file descriptor and a string,
// both of which are safe to send between threads
unsafe impl Send for VirtualKeyboard {}
unsafe impl Sync for VirtualKeyboard {}

#[cfg(test)]
mod tests {
    use super::*;
    
    // Mock VirtualKeyboard for testing without requiring actual hardware
    struct MockVirtualKeyboard {
        current_text: String,
        typed_chars: Vec<char>,
        backspace_count: usize,
        enter_pressed: bool,
    }
    
    impl MockVirtualKeyboard {
        fn new() -> Self {
            Self {
                current_text: String::new(),
                typed_chars: Vec::new(),
                backspace_count: 0,
                enter_pressed: false,
            }
        }
        
        fn type_text(&mut self, text: &str) -> Result<()> {
            for c in text.chars() {
                self.typed_chars.push(c);
            }
            Ok(())
        }
        
        fn press_backspace(&mut self) -> Result<()> {
            self.backspace_count += 1;
            if !self.typed_chars.is_empty() {
                self.typed_chars.pop();
            }
            Ok(())
        }
        
        fn press_enter(&mut self) -> Result<()> {
            self.enter_pressed = true;
            Ok(())
        }
        
        fn update_transcript(&mut self, new_transcript: &str) -> Result<()> {
            debug!("Updating transcript from '{}' to '{}'", self.current_text, new_transcript);

            // If the new transcript is empty, clear everything
            if new_transcript.is_empty() {
                self.clear_current_text()?;
                return Ok(());
            }

            // Check if the new transcript is just an extension of the current one
            if new_transcript.starts_with(&self.current_text) {
                // Just type the new characters
                let new_chars = &new_transcript[self.current_text.len()..];
                if !new_chars.is_empty() {
                    debug!("Typing new characters: '{}'", new_chars);
                    self.type_text(new_chars)?;
                    self.current_text = new_transcript.to_string();
                }
            } else {
                // Find the common prefix between current and new transcript
                let common_prefix_len = self.current_text
                    .chars()
                    .zip(new_transcript.chars())
                    .take_while(|(a, b)| a == b)
                    .count();
                
                let current_chars: Vec<char> = self.current_text.chars().collect();
                let chars_to_backspace = current_chars.len() - common_prefix_len;
                
                debug!("Common prefix length: {}, need to backspace {} characters", 
                       common_prefix_len, chars_to_backspace);
                
                // Only backspace the characters that differ
                if chars_to_backspace > 0 {
                    debug!("Backspacing {} characters", chars_to_backspace);
                    for _ in 0..chars_to_backspace {
                        self.press_backspace()?;
                    }
                }
                
                // Type the new ending (everything after the common prefix)
                let new_chars: Vec<char> = new_transcript.chars().collect();
                if common_prefix_len < new_chars.len() {
                    let new_ending: String = new_chars[common_prefix_len..].iter().collect();
                    debug!("Typing new ending: '{}'", new_ending);
                    self.type_text(&new_ending)?;
                }
                
                self.current_text = new_transcript.to_string();
            }

            Ok(())
        }
        
        fn finalize_transcript(&mut self) -> Result<()> {
            debug!("Finalizing transcript: '{}'", self.current_text);
            
            // Just clear the current text tracking, but don't press enter
            // The user can press enter manually when they're ready
            self.current_text.clear();
            
            Ok(())
        }
        
        fn clear_current_text(&mut self) -> Result<()> {
            if !self.current_text.is_empty() {
                self.backspace_current_text()?;
            }
            Ok(())
        }
        
        fn backspace_current_text(&mut self) -> Result<()> {
            let char_count = self.current_text.chars().count();
            debug!("Backspacing {} characters", char_count);
            
            for _ in 0..char_count {
                self.press_backspace()?;
            }
            
            self.current_text.clear();
            Ok(())
        }
    }
    
    #[test]
    fn test_incremental_typing_extension() {
        let mut kb = MockVirtualKeyboard::new();
        
        // Start with "hello"
        kb.update_transcript("hello").unwrap();
        assert_eq!(kb.current_text, "hello");
        assert_eq!(kb.typed_chars, ['h', 'e', 'l', 'l', 'o']);
        assert_eq!(kb.backspace_count, 0);
        
        // Extend to "hello world"
        kb.update_transcript("hello world").unwrap();
        assert_eq!(kb.current_text, "hello world");
        assert_eq!(kb.typed_chars, ['h', 'e', 'l', 'l', 'o', ' ', 'w', 'o', 'r', 'l', 'd']);
        assert_eq!(kb.backspace_count, 0);
    }
    
    #[test]
    fn test_incremental_typing_change() {
        let mut kb = MockVirtualKeyboard::new();
        
        // Start with "hello"
        kb.update_transcript("hello").unwrap();
        assert_eq!(kb.current_text, "hello");
        assert_eq!(kb.typed_chars, ['h', 'e', 'l', 'l', 'o']);
        
        // Change to "hi there" - should only backspace "ello" and type "i there"
        kb.update_transcript("hi there").unwrap();
        assert_eq!(kb.current_text, "hi there");
        // Should have backspaced 4 characters ("ello") and typed 7 new ones ("i there")
        assert_eq!(kb.backspace_count, 4);
        assert_eq!(kb.typed_chars, ['h', 'i', ' ', 't', 'h', 'e', 'r', 'e']);
    }
    
    #[test]
    fn test_finalize_transcript() {
        let mut kb = MockVirtualKeyboard::new();
        
        // Type some text
        kb.update_transcript("hello").unwrap();
        assert_eq!(kb.current_text, "hello");
        assert!(!kb.enter_pressed);
        
        // Finalize (should not press enter anymore)
        kb.finalize_transcript().unwrap();
        assert_eq!(kb.current_text, "");
        assert!(!kb.enter_pressed); // Should remain false
    }
    
    #[test]
    fn test_empty_transcript() {
        let mut kb = MockVirtualKeyboard::new();
        
        // Start with some text
        kb.update_transcript("hello").unwrap();
        assert_eq!(kb.current_text, "hello");
        
        // Clear with empty transcript
        kb.update_transcript("").unwrap();
        assert_eq!(kb.current_text, "");
        assert_eq!(kb.backspace_count, 5);
    }
    
    #[test]
    fn test_smart_backspacing_partial_change() {
        let mut kb = MockVirtualKeyboard::new();
        
        // Start with "hello world"
        kb.update_transcript("hello world").unwrap();
        assert_eq!(kb.current_text, "hello world");
        assert_eq!(kb.typed_chars, ['h', 'e', 'l', 'l', 'o', ' ', 'w', 'o', 'r', 'l', 'd']);
        
        // Change to "hello there" - should only backspace "world" and type "there"
        kb.update_transcript("hello there").unwrap();
        assert_eq!(kb.current_text, "hello there");
        // Should have backspaced 5 characters ("world") and typed 5 new ones ("there")
        assert_eq!(kb.backspace_count, 5);
        assert_eq!(kb.typed_chars, ['h', 'e', 'l', 'l', 'o', ' ', 't', 'h', 'e', 'r', 'e']);
    }
    
    #[test]
    fn test_smart_backspacing_no_common_prefix() {
        let mut kb = MockVirtualKeyboard::new();
        
        // Start with "hello"
        kb.update_transcript("hello").unwrap();
        assert_eq!(kb.current_text, "hello");
        
        // Change to "goodbye" - no common prefix, should backspace all and retype
        kb.update_transcript("goodbye").unwrap();
        assert_eq!(kb.current_text, "goodbye");
        // Should have backspaced 5 characters and typed 7 new ones
        assert_eq!(kb.backspace_count, 5);
        assert_eq!(kb.typed_chars, ['g', 'o', 'o', 'd', 'b', 'y', 'e']);
    }
    
    #[test]
    fn test_smart_backspacing_shortening() {
        let mut kb = MockVirtualKeyboard::new();
        
        // Start with "hello world"
        kb.update_transcript("hello world").unwrap();
        assert_eq!(kb.current_text, "hello world");
        
        // Change to "hello" - should only backspace " world"
        kb.update_transcript("hello").unwrap();
        assert_eq!(kb.current_text, "hello");
        // Should have backspaced 6 characters (" world") and typed 0 new ones
        assert_eq!(kb.backspace_count, 6);
        assert_eq!(kb.typed_chars, ['h', 'e', 'l', 'l', 'o']);
    }
}
