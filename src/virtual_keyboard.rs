use anyhow::{Context, Result};
use nix::fcntl::{open, OFlag};
use nix::sys::stat::Mode;
use nix::unistd::close;
use std::io::Write;
use std::os::unix::io::FromRawFd;
use tracing::{debug, error, info, warn};
use input_event_codes::*;
use nix::ioctl_write_ptr;
use std::fs::{File, OpenOptions};
use std::os::fd::AsRawFd;

use crate::input_event::*;

// Define ioctl macros for uinput
// The nix ioctl_write_int! macro requires the ioctl type and number
nix::ioctl_write_int!(ui_set_evbit, b'U', 100);
nix::ioctl_write_int!(ui_set_keybit, b'U', 101);
nix::ioctl_none!(ui_dev_create, b'U', 1);
nix::ioctl_none!(ui_dev_destroy, b'U', 2);

// UI_SET_EVBIT
const UI_SET_EVBIT: u8 = 0x40;
const UI_SET_KEYBIT: u8 = 0x41;
const UI_DEV_CREATE: u8 = 0x45;
const UI_DEV_DESTROY: u8 = 0x46;

#[repr(C)]
struct UInputUserDev {
    name: [u8; 80],
    id: UInputId,
    ff_effects_max: u32,
    absmax: [i32; 64],
    absmin: [i32; 64],
    absfuzz: [i32; 64],
    absflat: [i32; 64],
}

#[repr(C)]
#[derive(Default)]
struct UInputId {
    bustype: u16,
    vendor: u16,
    product: u16,
    version: u16,
}

pub struct VirtualKeyboard {
    fd: i32,
    name: String,
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

    #[allow(dead_code)]
    pub fn press_tab(&self) -> Result<()> {
        self.press_key(KEY_TAB)
    }

    #[allow(dead_code)]
    pub fn press_escape(&self) -> Result<()> {
        self.press_key(KEY_ESC)
    }

    #[allow(dead_code)]
    pub fn press_space(&self) -> Result<()> {
        self.press_key(KEY_SPACE)
    }

    // Special key combinations
    #[allow(dead_code)]
    pub fn press_ctrl_c(&self) -> Result<()> {
        self.send_key(KEY_LEFTCTRL, true)?;
        self.send_key(KEY_C, true)?;
        self.send_key(KEY_C, false)?;
        self.send_key(KEY_LEFTCTRL, false)?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn press_ctrl_v(&self) -> Result<()> {
        self.send_key(KEY_LEFTCTRL, true)?;
        self.send_key(KEY_V, true)?;
        self.send_key(KEY_V, false)?;
        self.send_key(KEY_LEFTCTRL, false)?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn press_alt_tab(&self) -> Result<()> {
        self.send_key(KEY_LEFTALT, true)?;
        self.send_key(KEY_TAB, true)?;
        self.send_key(KEY_TAB, false)?;
        self.send_key(KEY_LEFTALT, false)?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn get_name(&self) -> &str {
        &self.name
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
