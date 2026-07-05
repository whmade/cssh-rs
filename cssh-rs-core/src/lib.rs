//! Cross-platform cluster SSH tool

#![deny(clippy::implicit_return)]
#![allow(clippy::needless_return, clippy::doc_overindented_list_items)]
#![warn(missing_docs)]
#![doc(html_no_source)]
#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

use std::fs::{create_dir, File};
use std::mem;

use log::warn;
use registry::{value, Data, Hive, Security};
use simplelog::{format_description, ConfigBuilder, LevelFilter, WriteLogger};
use windows::core::PWSTR;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Threading::{PROCESS_INFORMATION, STARTUPINFOW};

#[cfg(test)]
use mockall::automock;

pub mod cli;
pub mod client;
pub mod daemon;
pub mod utils;

use utils::windows::WindowsApi;

/// CLSID identifying `conhost.exe` in the registry.
///
/// As used in Windows Terminal:
/// <https://github.com/microsoft/terminal/blob/v1.22.3232.0/src/propslib/DelegationConfig.hpp#L105>
const CLSID_CONHOST: &str = "{B23D10C0-E52E-411E-9D5B-C09FDF709C7D}";
/// Registry path where `DelegationConsole` and `DelegationTerminal` registry keys are stored.
///
/// These registry keys store the configuration value for the default terminal application.
const DEFAULT_TERMINAL_APP_REGISTRY_PATH: &str = r"Console\%%Startup";
/// `DelegationConsole` registry key.
///
/// As used in Windows Terminal:
/// <https://github.com/microsoft/terminal/blob/v1.22.3232.0/src/propslib/DelegationConfig.cpp#L29>
const DELEGATION_CONSOLE: &str = "DelegationConsole";
/// `DelegationTerminal` registry key.
///
/// As used in Windows Terminal:
/// <https://github.com/microsoft/terminal/blob/v1.22.3232.0/src/propslib/DelegationConfig.cpp#L30>
const DELEGATION_TERMINAL: &str = "DelegationTerminal";

/// Trait for registry operations to enable mocking in tests
#[cfg_attr(test, automock)]
pub trait Registry {
    /// Return the string value at `path`/`name`, or `None` if the key or value
    /// does not exist (or cannot be read).
    fn get_registry_string_value(&self, path: &str, name: &str) -> Option<String>;
    /// Set a string value, creating the key if it does not exist. Return whether it succeeded.
    fn set_registry_string_value(&self, path: &str, name: &str, value: &str) -> bool;
    /// Delete a value. Return `true` when the value is absent afterwards (an
    /// already-absent value counts as success).
    fn delete_registry_string_value(&self, path: &str, name: &str) -> bool;
    /// Return whether the registry key at `path` exists.
    fn registry_key_exists(&self, path: &str) -> bool;
    /// Delete the registry key at `path` recursively. Return whether it succeeded.
    fn delete_registry_key(&self, path: &str) -> bool;
}

/// Default implementation of Registry trait that performs actual Windows registry API calls
pub struct DefaultRegistry;

#[cfg_attr(coverage_nightly, coverage(off))]
impl Registry for DefaultRegistry {
    fn get_registry_string_value(&self, path: &str, name: &str) -> Option<String> {
        let key = Hive::CurrentUser
            .open(path, Security::Read | Security::Write)
            .ok()?;
        match key.value(name) {
            Ok(Data::String(value)) => return Some(value.to_string_lossy()),
            Ok(_) => panic!("Expected string data for {name} registry value"),
            Err(value::Error::NotFound(_, _)) => return None,
            Err(err) => {
                warn!("Failed to read {} value from registry: {}", name, err);
                return None;
            }
        }
    }

    fn set_registry_string_value(&self, path: &str, name: &str, value: &str) -> bool {
        // create opens the key or creates it (with any missing parents) if absent,
        // so the guard can force conhost on a profile that never set a default terminal.
        match Hive::CurrentUser.create(path, Security::Read | Security::Write) {
            Ok(key) => match key.set_value::<String>(
                name.to_owned(),
                &Data::String(value.to_owned().try_into().unwrap()),
            ) {
                Ok(()) => return true,
                Err(_) => {
                    warn!("Failed to set registry value {} to {}", name, value);
                    return false;
                }
            },
            Err(_) => {
                warn!("Failed to open or create registry key {}", path);
                return false;
            }
        }
    }

    fn delete_registry_string_value(&self, path: &str, name: &str) -> bool {
        let key = match Hive::CurrentUser.open(path, Security::Read | Security::Write) {
            Ok(key) => key,
            // No key means the value is already absent.
            Err(_) => return true,
        };
        match key.delete_value(name) {
            Ok(()) => return true,
            Err(value::Error::NotFound(_, _)) => return true,
            Err(err) => {
                warn!("Failed to delete registry value {}: {}", name, err);
                return false;
            }
        }
    }

    fn registry_key_exists(&self, path: &str) -> bool {
        return Hive::CurrentUser.open(path, Security::Read).is_ok();
    }

    fn delete_registry_key(&self, path: &str) -> bool {
        match Hive::CurrentUser.delete(path, true) {
            Ok(()) => return true,
            Err(err) => {
                warn!("Failed to delete registry key {}: {}", path, err);
                return false;
            }
        }
    }
}

/// Return the Window Handle [HWND] for the foreground window associated with the given `process_id`.
///
/// If multiple foreground windows are associated with the given `process_id` it is undefined which [HWND] gets returned.
///
/// # Arguments
///
/// * `windows_api` - Windows API operations implementation
/// * `process_id` - ID of the process for which to retrieve the window handle.
///
/// # Returns
///
/// The Window Handle [HWND] for the window associated with the given `process_id`.
pub fn get_console_window_handle<W: WindowsApi>(windows_api: &W, process_id: u32) -> HWND {
    return windows_api.get_window_handle_for_process(process_id);
}

/// Create process with command line using the provided API (testable version)
///
/// # Arguments
///
/// * `api` - Windows API operations implementation
/// * `application` - Application name including file extension
/// * `command_line` - UTF-16 encoded command line
///
/// # Returns
///
/// [PROCESS_INFORMATION] of the spawned process or None if failed
pub fn create_process<W: WindowsApi>(
    api: &W,
    application: &str,
    command_line: &[u16],
) -> Option<PROCESS_INFORMATION> {
    let mut startupinfo = STARTUPINFOW {
        cb: mem::size_of::<STARTUPINFOW>() as u32,
        ..Default::default()
    };
    let mut process_information = PROCESS_INFORMATION::default();
    let mut cmd_line = command_line.to_vec();
    let command_line_ptr = PWSTR(cmd_line.as_mut_ptr());

    match api.create_process_raw(
        application,
        command_line_ptr,
        &mut startupinfo,
        &mut process_information,
    ) {
        Ok(()) => return Some(process_information),
        Err(_) => return None,
    }
}

/// Trait for file system operations to enable mocking in tests
#[cfg_attr(test, automock)]
pub trait FileSystem {
    /// Create a directory
    fn create_directory(&self, path: &str) -> bool;
    /// Create a log file
    fn create_log_file(&self, filename: &str) -> bool;
}

/// Default implementation of FileSystem trait that performs actual file system operations
pub struct ProductionFileSystem;

#[cfg_attr(coverage_nightly, coverage(off))]
impl FileSystem for ProductionFileSystem {
    fn create_directory(&self, path: &str) -> bool {
        return create_dir(path).is_ok() || std::path::Path::new(path).exists();
    }

    fn create_log_file(&self, filename: &str) -> bool {
        return File::create(filename).is_ok();
    }
}

/// Previous state of a registry value the guard overrode, used to undo it on drop.
#[derive(Debug, PartialEq, Eq)]
enum PreviousValue {
    /// The value existed with this string; the guard restores it on drop.
    Existing(String),
    /// The value did not exist; the guard deletes it again on drop.
    Absent,
}

/// Map a read registry value to the previous state the guard must restore on drop.
fn previous_value(value: Option<String>) -> PreviousValue {
    match value {
        Some(value) => return PreviousValue::Existing(value),
        None => return PreviousValue::Absent,
    }
}

/// Guard that configures `conhost.exe` as the default terminal application and
/// fully reverts its changes when dropped.
///
/// Restoration is exact: values the guard overwrote are set back, values it
/// created are deleted, and a startup key the guard had to create is removed.
/// The key is created when absent - a fresh Windows profile that never chose a
/// default terminal has no `%%Startup` key, so without this cssh would silently
/// leave the OS default (Windows Terminal) in place.
pub struct WindowsSettingsDefaultTerminalApplicationGuard<R: Registry> {
    /// How to undo `DelegationConsole` on drop; `None` when the guard made no change.
    console: Option<PreviousValue>,
    /// How to undo `DelegationTerminal` on drop; `None` when the guard made no change.
    terminal: Option<PreviousValue>,
    /// Whether the startup key existed before the guard. When it did not, the
    /// guard created it and deletes it on drop.
    key_existed: bool,
    /// Registry operations trait
    registry: R,
}

impl<R: Registry> WindowsSettingsDefaultTerminalApplicationGuard<R> {
    /// Create a new guard, forcing `conhost.exe` as the default terminal application.
    ///
    /// Creates the startup key and values when absent and records how to undo
    /// each change on drop.
    ///
    /// # Arguments
    ///
    /// * `registry` - Registry operations implementation
    ///
    /// # Returns
    ///
    /// A new guard that reverts its registry changes on drop.
    pub fn new_with_registry(registry: R) -> Self {
        let key_existed = registry.registry_key_exists(DEFAULT_TERMINAL_APP_REGISTRY_PATH);
        let console_value = registry
            .get_registry_string_value(DEFAULT_TERMINAL_APP_REGISTRY_PATH, DELEGATION_CONSOLE);
        let terminal_value = registry
            .get_registry_string_value(DEFAULT_TERMINAL_APP_REGISTRY_PATH, DELEGATION_TERMINAL);

        // Nothing to change or undo when conhost is already the default terminal.
        if console_value.as_deref() == Some(CLSID_CONHOST)
            && terminal_value.as_deref() == Some(CLSID_CONHOST)
        {
            return WindowsSettingsDefaultTerminalApplicationGuard {
                console: None,
                terminal: None,
                key_existed,
                registry,
            };
        }

        let guard = WindowsSettingsDefaultTerminalApplicationGuard {
            console: Some(previous_value(console_value)),
            terminal: Some(previous_value(terminal_value)),
            key_existed,
            registry,
        };

        guard.registry.set_registry_string_value(
            DEFAULT_TERMINAL_APP_REGISTRY_PATH,
            DELEGATION_CONSOLE,
            CLSID_CONHOST,
        );
        guard.registry.set_registry_string_value(
            DEFAULT_TERMINAL_APP_REGISTRY_PATH,
            DELEGATION_TERMINAL,
            CLSID_CONHOST,
        );

        return guard;
    }

    /// Undo one value: restore its previous string, or delete it if it was absent.
    ///
    /// # Arguments
    ///
    /// * `name`     - Registry value name to undo.
    /// * `previous` - The value's state before the guard changed it.
    fn restore_value(&self, name: &str, previous: &PreviousValue) {
        match previous {
            PreviousValue::Existing(value) => {
                self.registry.set_registry_string_value(
                    DEFAULT_TERMINAL_APP_REGISTRY_PATH,
                    name,
                    value,
                );
            }
            PreviousValue::Absent => {
                self.registry
                    .delete_registry_string_value(DEFAULT_TERMINAL_APP_REGISTRY_PATH, name);
            }
        };
    }
}

impl WindowsSettingsDefaultTerminalApplicationGuard<DefaultRegistry> {
    /// Create a new guard with production registry operations
    pub fn new() -> Self {
        return Self::new_with_registry(DefaultRegistry);
    }
}

impl<R: Registry> Default for WindowsSettingsDefaultTerminalApplicationGuard<R>
where
    R: Default,
{
    fn default() -> Self {
        return Self::new_with_registry(R::default());
    }
}

impl Default for DefaultRegistry {
    fn default() -> Self {
        return DefaultRegistry;
    }
}

impl<R: Registry> Drop for WindowsSettingsDefaultTerminalApplicationGuard<R> {
    /// Revert every registry change the guard made: delete a startup key the
    /// guard created (which removes the values it wrote), otherwise restore
    /// overwritten values and delete values it created.
    fn drop(&mut self) {
        // The guard made no change - conhost was already the default terminal.
        if self.console.is_none() && self.terminal.is_none() {
            return;
        }
        // The guard created the startup key; deleting it removes the values too.
        if !self.key_existed {
            self.registry
                .delete_registry_key(DEFAULT_TERMINAL_APP_REGISTRY_PATH);
            return;
        }
        if let Some(previous) = &self.console {
            self.restore_value(DELEGATION_CONSOLE, previous);
        }
        if let Some(previous) = &self.terminal {
            self.restore_value(DELEGATION_TERMINAL, previous);
        }
    }
}

/// Launch the given console application with the given arguments as a new detached process with its own console window.
///
/// Input/Output handles are not being inherited.
/// Whichever default terminal application is configured in the windows system settings will be used
/// to host the application (i.e. create the window).
///
/// # Arguments
///
/// * `api`                 - Windows API implementation
/// * `application`         - Application name including file extension (`.exe`).
///                           If the application is not in the `PATH` environment variable,
///                           the full path must be specified.
/// * `args`                - List of arguments to the application.
/// * `with_keyboard_focus` - Whether the new console window should take foreground focus
///                           when it appears. Pass `false` when spawning child consoles
///                           that must not steal focus from the calling process.
///
/// # Returns
///
/// [PROCESS_INFORMATION] of the spawned process.
pub fn spawn_console_process<W: WindowsApi>(
    api: &W,
    application: &str,
    args: Vec<String>,
    with_keyboard_focus: bool,
) -> Option<PROCESS_INFORMATION> {
    return api.create_process_with_args(application, args, with_keyboard_focus);
}

/// Return the path to the currently running executable.
///
/// Used when spawning child daemon/client consoles so that they invoke the same
/// binary that is currently running, regardless of how the user has named the
/// executable on disk. Hard-coding `cssh-rs.exe` would break any deployment that
/// renames the binary (e.g. release artifacts that embed the version number).
///
/// # Returns
///
/// The current executable path as a UTF-8 string. The conversion is lossy if
/// the path contains non-UTF-8 code units.
///
/// # Panics
///
/// Panics if `std::env::current_exe()` fails. The standard library only
/// returns an error in highly unusual circumstances (e.g. the executable has
/// been deleted while running); the caller cannot meaningfully recover.
pub fn current_exe_path() -> String {
    return std::env::current_exe()
        .expect("Failed to determine current executable path")
        .to_string_lossy()
        .into_owned();
}

/// Initialize the logger.
///
/// Makes sure a `logs` directory exists in the current working directory.
/// Log filename format: `<utc-time-of-executable-start>_<name>.log`.
/// Configures [log_panics].
///
/// # Arguments
///
/// * `name` - Will be part of the log filename.
pub fn init_logger(name: &str) {
    init_logger_with_fs(&ProductionFileSystem, name);
}

/// Initialize the logger with the provided file system operations.
///
/// # Arguments
///
/// * `fs` - File system operations implementation
/// * `name` - Will be part of the log filename
pub fn init_logger_with_fs<F: FileSystem>(fs: &F, name: &str) {
    let utc_now = chrono::offset::Utc::now()
        .format("%Y-%m-%d_%H-%M-%S.%f")
        .to_string();

    fs.create_directory("logs");

    let filename = format!("logs/{utc_now}_{name}.log");
    if fs.create_log_file(&filename) {
        if let Ok(file) = File::create(&filename) {
            let _ = WriteLogger::init(
                LevelFilter::Debug,
                ConfigBuilder::new()
                    .set_time_format_custom(format_description!(
                        "[hour]:[minute]:[second].[subsecond]"
                    ))
                    .build(),
                file,
            );
            log_panics::init();
        }
    }
}

/// Detect if application was launched from Windows Explorer (GUI) vs command line using the provided console API.
///
/// Returns true if launched from GUI (separate console), false if from existing console.
/// Based on: <https://devblogs.microsoft.com/oldnewthing/20160125-00/?p=92922>
///
/// # Arguments
///
/// * `windows_api` - Windows API operations implementation
///
/// # Returns
///
/// * `true` - Application was launched from GUI (Explorer, double-click, etc.)
/// * `false` - Application was launched from existing console (command line)
pub fn is_launched_from_gui<W: WindowsApi>(windows_api: &W) -> bool {
    return windows_api.get_console_attached_process_count() == 1;
}

#[cfg(test)]
#[path = "./tests/test_lib.rs"]
mod test_lib;
