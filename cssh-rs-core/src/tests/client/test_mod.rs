//! Unit tests for the client module.

use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;

use tokio::sync::watch;
use windows::Win32::Foundation::COLORREF;
use windows::Win32::System::Console::{CONSOLE_CHARACTER_ATTRIBUTES, CONSOLE_SCREEN_BUFFER_INFOEX};

use crate::client::{
    build_ssh_arguments, get_effective_color, get_flash_color, paint_console_color,
    resolve_username, run_visuals_loop, ClientState, ConsolePaint,
};
use crate::utils::config::ClientConfig;
use crate::utils::windows::{ConsolePaletteSnapshot, MockWindowsApi};
// Test constants - consistent dummy values used throughout tests
const TEST_USERNAME: &str = "testuser";
const TEST_HOSTNAME: &str = "example.com";
const TEST_PLACEHOLDER: &str = "{{USERNAME_AT_HOST}}";
const TEST_SSH_PROGRAM: &str = "ssh";

/// Creates a test ClientConfig with the given SSH config path.
///
/// # Arguments
///
/// * `ssh_config_path` - Path to the SSH config file.
///
/// # Returns
///
/// A ClientConfig instance for testing.
fn create_test_client_config(ssh_config_path: String) -> ClientConfig {
    return ClientConfig {
        ssh_config_path,
        program: TEST_SSH_PROGRAM.to_string(),
        arguments: vec!["-XY".to_string(), TEST_PLACEHOLDER.to_string()],
        username_host_placeholder: TEST_PLACEHOLDER.to_string(),
        disabled_console_color: ClientConfig::default().disabled_console_color,
        highlighted_console_color: ClientConfig::default().highlighted_console_color,
    };
}

/// Creates a temporary SSH config file for testing.
///
/// # Arguments
///
/// * `content` - The content to write to the SSH config file.
///
/// # Returns
///
/// A tuple containing the temporary directory path and the path to the SSH config file.
fn create_temp_ssh_config(content: &str) -> (PathBuf, String) {
    let temp_dir = env::temp_dir().join(format!("cssh-rs_test_{}", std::process::id()));
    fs::create_dir_all(&temp_dir).expect("Failed to create temporary directory");
    let config_path = temp_dir.join("config");
    let mut file = File::create(&config_path).expect("Failed to create SSH config file");
    file.write_all(content.as_bytes())
        .expect("Failed to write SSH config content");
    let config_path_str = config_path.to_string_lossy().to_string();
    return (temp_dir, config_path_str);
}

#[test]
fn test_resolve_username_basic_scenarios() {
    let config = create_test_client_config("/nonexistent/path".to_string());

    // Test with provided username
    let result = resolve_username(Some(TEST_USERNAME.to_string()), TEST_HOSTNAME, &config);
    assert_eq!(result, TEST_USERNAME);

    // Test without username and no SSH config
    let result = resolve_username(None, TEST_HOSTNAME, &config);
    assert_eq!(result, "");

    // Test edge cases
    let result = resolve_username(Some(TEST_USERNAME.to_string()), "", &config);
    assert_eq!(result, TEST_USERNAME);

    let result = resolve_username(None, "", &config);
    assert_eq!(result, "");
}

#[test]
fn test_resolve_username_ssh_config_integration() {
    // Test that provided username always overrides SSH config
    let ssh_config_content = format!("Host {TEST_HOSTNAME}\n    User configuser\n");
    let (_temp_dir, config_path) = create_temp_ssh_config(&ssh_config_content);
    let config = create_test_client_config(config_path);

    let result = resolve_username(Some(TEST_USERNAME.to_string()), TEST_HOSTNAME, &config);
    assert_eq!(result, TEST_USERNAME);

    // Test SSH config parsing integration
    let result = resolve_username(None, TEST_HOSTNAME, &config);
    assert_eq!(result, "configuser");

    // Test empty SSH config
    let (_temp_dir, empty_config_path) = create_temp_ssh_config("");
    let empty_config = create_test_client_config(empty_config_path);
    let result = resolve_username(None, TEST_HOSTNAME, &empty_config);
    assert_eq!(result, "");
}

#[test]
fn test_resolve_username_special_characters() {
    let config = create_test_client_config("/nonexistent/path".to_string());

    // Test various special characters that might appear in usernames/hostnames
    let test_cases = [
        ("user.name", "sub.example.com", "user.name"),
        ("user-name", "host-name", "user-name"),
        ("user_name", "host_name", "user_name"),
        ("t\u{eb}st", "ex\u{e4}mple.com", "t\u{eb}st"), // Unicode
        (TEST_USERNAME, "host name", TEST_USERNAME),    // Whitespace in hostname
    ];

    for (username, hostname, expected) in test_cases {
        let result = resolve_username(Some(username.to_string()), hostname, &config);
        assert_eq!(result, expected);
    }
}

/// Test case structure for build_ssh_arguments function.
struct SshArgumentsTestCase<'a> {
    /// Description of what this test case is testing.
    description: &'a str,
    /// Username to test.
    username: &'a str,
    /// Hostname to test.
    host: &'a str,
    /// Optional port to test.
    port: Option<u16>,
    /// Configuration to use for the test.
    config: &'a ClientConfig,
    /// Expected output arguments.
    expected_output: Vec<String>,
}

#[test]
fn test_build_ssh_arguments() {
    let config = create_test_client_config("/nonexistent/path".to_string());
    let complex_config = ClientConfig {
        ssh_config_path: "/nonexistent/path".to_string(),
        program: TEST_SSH_PROGRAM.to_string(),
        arguments: vec![
            "-v".to_string(),
            "-o".to_string(),
            "StrictHostKeyChecking=no".to_string(),
            TEST_PLACEHOLDER.to_string(),
            "-X".to_string(),
        ],
        username_host_placeholder: TEST_PLACEHOLDER.to_string(),
        disabled_console_color: ClientConfig::default().disabled_console_color,
        highlighted_console_color: ClientConfig::default().highlighted_console_color,
    };

    let test_cases = [
        SshArgumentsTestCase {
            description: "basic case without port",
            username: TEST_USERNAME,
            host: TEST_HOSTNAME,
            port: None,
            config: &config,
            expected_output: vec![
                "-XY".to_string(),
                format!("{TEST_USERNAME}@{TEST_HOSTNAME}"),
            ],
        },
        SshArgumentsTestCase {
            description: "empty username and host",
            username: "",
            host: "",
            port: None,
            config: &config,
            expected_output: vec!["-XY".to_string(), "@".to_string()],
        },
        SshArgumentsTestCase {
            description: "complex arguments without port",
            username: TEST_USERNAME,
            host: TEST_HOSTNAME,
            port: None,
            config: &complex_config,
            expected_output: vec![
                "-v".to_string(),
                "-o".to_string(),
                "StrictHostKeyChecking=no".to_string(),
                format!("{TEST_USERNAME}@{TEST_HOSTNAME}"),
                "-X".to_string(),
            ],
        },
        // Cases with port
        SshArgumentsTestCase {
            description: "basic case with port 2222",
            username: TEST_USERNAME,
            host: TEST_HOSTNAME,
            port: Some(2222),
            config: &config,
            expected_output: vec![
                "-XY".to_string(),
                format!("{TEST_USERNAME}@{TEST_HOSTNAME}"),
                "-p".to_string(),
                "2222".to_string(),
            ],
        },
        SshArgumentsTestCase {
            description: "standard SSH port 22",
            username: TEST_USERNAME,
            host: TEST_HOSTNAME,
            port: Some(22),
            config: &config,
            expected_output: vec![
                "-XY".to_string(),
                format!("{TEST_USERNAME}@{TEST_HOSTNAME}"),
                "-p".to_string(),
                "22".to_string(),
            ],
        },
        SshArgumentsTestCase {
            description: "high port number",
            username: TEST_USERNAME,
            host: TEST_HOSTNAME,
            port: Some(65535),
            config: &config,
            expected_output: vec![
                "-XY".to_string(),
                format!("{TEST_USERNAME}@{TEST_HOSTNAME}"),
                "-p".to_string(),
                "65535".to_string(),
            ],
        },
        SshArgumentsTestCase {
            description: "complex arguments with port",
            username: TEST_USERNAME,
            host: TEST_HOSTNAME,
            port: Some(8080),
            config: &complex_config,
            expected_output: vec![
                "-v".to_string(),
                "-o".to_string(),
                "StrictHostKeyChecking=no".to_string(),
                format!("{TEST_USERNAME}@{TEST_HOSTNAME}"),
                "-X".to_string(),
                "-p".to_string(),
                "8080".to_string(),
            ],
        },
        // Special characters
        SshArgumentsTestCase {
            description: "hostname with dashes and port",
            username: "user",
            host: "host-name.example.com",
            port: Some(2222),
            config: &config,
            expected_output: vec![
                "-XY".to_string(),
                "user@host-name.example.com".to_string(),
                "-p".to_string(),
                "2222".to_string(),
            ],
        },
        SshArgumentsTestCase {
            description: "IP address with port",
            username: "user",
            host: "192.168.1.1",
            port: Some(8080),
            config: &config,
            expected_output: vec![
                "-XY".to_string(),
                "user@192.168.1.1".to_string(),
                "-p".to_string(),
                "8080".to_string(),
            ],
        },
        SshArgumentsTestCase {
            description: "IPv6 address with port",
            username: "user",
            host: "[::1]",
            port: Some(2222),
            config: &config,
            expected_output: vec![
                "-XY".to_string(),
                "user@[::1]".to_string(),
                "-p".to_string(),
                "2222".to_string(),
            ],
        },
        SshArgumentsTestCase {
            description: "underscores in username and hostname",
            username: "test_user",
            host: "test_host",
            port: Some(9999),
            config: &config,
            expected_output: vec![
                "-XY".to_string(),
                "test_user@test_host".to_string(),
                "-p".to_string(),
                "9999".to_string(),
            ],
        },
        SshArgumentsTestCase {
            description: "dots in username and hostname",
            username: "user.name",
            host: "host.name",
            port: Some(1234),
            config: &config,
            expected_output: vec![
                "-XY".to_string(),
                "user.name@host.name".to_string(),
                "-p".to_string(),
                "1234".to_string(),
            ],
        },
    ];

    for test_case in test_cases {
        let result = build_ssh_arguments(
            test_case.username,
            test_case.host,
            test_case.port,
            test_case.config,
        );
        assert_eq!(
            result, test_case.expected_output,
            "Failed test case: {}",
            test_case.description
        );
    }
}

#[test]
fn test_get_effective_color_covers_state_and_highlight_matrix() {
    let disabled = CONSOLE_CHARACTER_ATTRIBUTES(0x87);
    let highlighted = CONSOLE_CHARACTER_ATTRIBUTES(0x1F);
    let test_cases = [
        (
            "active unhighlighted",
            ClientState::Active,
            false,
            ConsolePaint::Restore,
        ),
        (
            "disabled unhighlighted",
            ClientState::Disabled,
            false,
            ConsolePaint::Tint(disabled),
        ),
        (
            "highlighted active",
            ClientState::Active,
            true,
            ConsolePaint::Tint(highlighted),
        ),
        (
            "highlighted disabled",
            ClientState::Disabled,
            true,
            ConsolePaint::Tint(highlighted),
        ),
    ];
    for (description, state, is_highlighted, expected) in test_cases {
        assert_eq!(
            get_effective_color(state, is_highlighted, disabled, highlighted),
            expected,
            "{description}"
        );
    }
}

#[test]
fn test_get_flash_color_covers_state_matrix() {
    let disabled = CONSOLE_CHARACTER_ATTRIBUTES(0x87);
    let test_cases = [
        ("active", ClientState::Active, ConsolePaint::Restore),
        (
            "disabled",
            ClientState::Disabled,
            ConsolePaint::Tint(disabled),
        ),
    ];
    for (description, state, expected) in test_cases {
        assert_eq!(get_flash_color(state, disabled), expected, "{description}");
    }
}

/// A palette with 16 distinct entries so remaps are observable.
fn sample_palette() -> [COLORREF; 16] {
    return core::array::from_fn(|index| return COLORREF(0x0000_1000 * (index as u32 + 1)));
}

/// Build a `CONSOLE_SCREEN_BUFFER_INFOEX` carrying `palette`.
fn palette_info(palette: [COLORREF; 16]) -> CONSOLE_SCREEN_BUFFER_INFOEX {
    return CONSOLE_SCREEN_BUFFER_INFOEX {
        ColorTable: palette,
        ..Default::default()
    };
}

/// Build the snapshot `snapshot_console_palette` yields for `palette_info`
/// (default attribute `0`, matching the info builder above).
fn palette_snapshot(palette: [COLORREF; 16]) -> ConsolePaletteSnapshot {
    return ConsolePaletteSnapshot {
        color_table: palette,
        default_attributes: CONSOLE_CHARACTER_ATTRIBUTES(0),
    };
}

/// Expect `reads` palette reads (all returning `palette`) and `writes` palette
/// writes (all succeeding).
fn expect_palette_ops(
    mock: &mut MockWindowsApi,
    palette: [COLORREF; 16],
    reads: usize,
    writes: usize,
) {
    mock.expect_get_console_screen_buffer_info_ex()
        .times(reads)
        .returning(move || return Ok(palette_info(palette)));
    mock.expect_set_console_screen_buffer_info_ex()
        .times(writes)
        .returning(|_| return Ok(()));
    // Each palette write forces a repaint so the recolor shows immediately.
    mock.expect_invalidate_console_window()
        .times(writes)
        .returning(|| return Ok(()));
}

#[test]
fn test_paint_console_color_tint_applies_tinted_palette() {
    // A tint from the restored state installs the tinted palette (one read + one
    // write inside `set_console_palette`).
    let base = palette_snapshot(sample_palette());
    let mut mock_api = MockWindowsApi::new();
    expect_palette_ops(&mut mock_api, base.color_table, 1, 1);

    let target = ConsolePaint::Tint(CONSOLE_CHARACTER_ATTRIBUTES(0x1F));
    let mut last: Option<ConsolePaint> = Some(ConsolePaint::Restore);

    paint_console_color(&mock_api, target, Some(&base), &mut last);

    assert_eq!(last, Some(target));
}

#[test]
fn test_paint_console_color_tint_to_tint_applies_new_color() {
    // A different tint replaces the current one, installing the new tinted
    // palette from the same pristine base.
    let base = palette_snapshot(sample_palette());
    let mut mock_api = MockWindowsApi::new();
    expect_palette_ops(&mut mock_api, base.color_table, 1, 1);

    let mut last: Option<ConsolePaint> =
        Some(ConsolePaint::Tint(CONSOLE_CHARACTER_ATTRIBUTES(0x8F)));
    let target = ConsolePaint::Tint(CONSOLE_CHARACTER_ATTRIBUTES(0x1F));

    paint_console_color(&mock_api, target, Some(&base), &mut last);

    assert_eq!(last, Some(target));
}

#[test]
fn test_paint_console_color_no_base_is_noop() {
    // With no captured base (the palette was unreadable at startup) painting is
    // suppressed and `last` is left unchanged. No mock calls.
    let mock_api = MockWindowsApi::new();
    let target = ConsolePaint::Tint(CONSOLE_CHARACTER_ATTRIBUTES(0x1F));
    let mut last: Option<ConsolePaint> = Some(ConsolePaint::Restore);

    paint_console_color(&mock_api, target, None, &mut last);

    assert_eq!(last, Some(ConsolePaint::Restore));
}

#[test]
fn test_paint_console_color_write_failure_keeps_last_for_retry() {
    // If the palette write fails, `last` must be left as it was so the next
    // transition retries, rather than recording a paint that never reached the
    // screen (which would strand the window in the tint).
    let base = palette_snapshot(sample_palette());
    let mut mock_api = MockWindowsApi::new();
    mock_api
        .expect_get_console_screen_buffer_info_ex()
        .times(1)
        .returning(move || return Ok(palette_info(base.color_table)));
    mock_api
        .expect_set_console_screen_buffer_info_ex()
        .times(1)
        .returning(|_| return Err(windows::core::Error::from_thread()));
    // No invalidate: a failed write must not reach the repaint.

    let tinted = ConsolePaint::Tint(CONSOLE_CHARACTER_ATTRIBUTES(0x1F));
    let mut last: Option<ConsolePaint> = Some(tinted);

    paint_console_color(&mock_api, ConsolePaint::Restore, Some(&base), &mut last);

    assert_eq!(last, Some(tinted));
}

#[test]
fn test_paint_console_color_restore_writes_palette_back() {
    // Returning to the enabled look writes the pristine base palette back.
    let base = palette_snapshot(sample_palette());
    let mut mock_api = MockWindowsApi::new();
    expect_palette_ops(&mut mock_api, base.color_table, 1, 1);

    let mut last: Option<ConsolePaint> =
        Some(ConsolePaint::Tint(CONSOLE_CHARACTER_ATTRIBUTES(0x1F)));

    paint_console_color(&mock_api, ConsolePaint::Restore, Some(&base), &mut last);

    assert_eq!(last, Some(ConsolePaint::Restore));
}

#[test]
fn test_paint_console_color_restore_is_noop_when_not_tinted() {
    // A restore while already restored (the client never tinted) is skipped by
    // the equality guard, so the pristine palette is not needlessly rewritten.
    // No mock calls.
    let base = palette_snapshot(sample_palette());
    let mock_api = MockWindowsApi::new();
    let mut last: Option<ConsolePaint> = Some(ConsolePaint::Restore);

    paint_console_color(&mock_api, ConsolePaint::Restore, Some(&base), &mut last);

    assert_eq!(last, Some(ConsolePaint::Restore));
}

#[test]
fn test_paint_console_color_skips_when_target_matches_last() {
    // No mock expectations: an unchanged intent must not touch the console.
    // `MockWindowsApi` would panic on any unexpected call.
    let base = palette_snapshot(sample_palette());
    let mock_api = MockWindowsApi::new();
    let same = ConsolePaint::Tint(CONSOLE_CHARACTER_ATTRIBUTES(0x1F));
    let mut last: Option<ConsolePaint> = Some(same);

    paint_console_color(&mock_api, same, Some(&base), &mut last);

    assert_eq!(last, Some(same));
}

/// Adds the palette-op expectations for the flash sequence a highlighted client
/// runs: an initial tint, a flash restore, and a re-tint. Each of the 3 paints
/// reads then writes the buffer info inside `set_console_palette`.
fn expect_flash_palette_ops(mock: &mut MockWindowsApi) {
    let base = sample_palette();
    mock.expect_get_console_screen_buffer_info_ex()
        .times(3)
        .returning(move || return Ok(palette_info(base)));
    mock.expect_set_console_screen_buffer_info_ex()
        .times(3)
        .returning(|_| return Ok(()));
    mock.expect_invalidate_console_window()
        .times(3)
        .returning(|| return Ok(()));
}

/// Regression test for the action-feedback flash on idempotent
/// submenu actions: pressing `[e]` on an already-Active highlighted
/// client (or `[d]` on already-Disabled) must still flash the
/// underlying state color, even though `ClientState` did not
/// actually change. The Active flash restores the saved palette.
#[tokio::test]
async fn test_visuals_flash_on_same_value_state_push_while_highlighted() {
    let disabled = CONSOLE_CHARACTER_ATTRIBUTES(0x8F);
    let highlighted = CONSOLE_CHARACTER_ATTRIBUTES(0x1F);

    // Paint sequence:
    // 1) initial steady-state -> Tint(highlighted): apply.
    // 2) flash after same-Active push -> Restore: write the base palette back.
    // 3) flash deadline elapses -> Tint(highlighted) again: apply.
    let mut mock_api = MockWindowsApi::new();
    expect_flash_palette_ops(&mut mock_api);

    let (state_sender, state_receiver) = watch::channel(ClientState::Active);
    let (highlight_sender, highlight_receiver) = watch::channel(true);

    let visuals = run_visuals_loop(
        &mock_api,
        state_receiver,
        highlight_receiver,
        Some(palette_snapshot(sample_palette())),
        disabled,
        highlighted,
    );
    let driver = async {
        // Let the loop apply the initial paint.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        // Same value: would be a no-op under the old equality guard.
        state_sender.send_replace(ClientState::Active);
        // Wait past `HIGHLIGHT_FLASH_DURATION` (250 ms) so the flash
        // deadline elapses and the steady-state highlight is restored.
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        // Drop both senders to terminate the visuals loop.
        drop(state_sender);
        drop(highlight_sender);
    };

    tokio::join!(visuals, driver);
    // `mock_api` drops here; mockall's `Drop` impl panics if the
    // expectations were not exactly satisfied.
}

/// Core regression test for GH #279: disabling then re-enabling a client tints
/// via the palette and restores the pristine palette on re-enable, never
/// touching the per-cell buffer that holds the child's (possibly truecolor)
/// output.
#[tokio::test]
async fn test_visuals_disable_enable_restores_palette() {
    let disabled = CONSOLE_CHARACTER_ATTRIBUTES(0x8F);
    let highlighted = CONSOLE_CHARACTER_ATTRIBUTES(0x1F);

    // Start unhighlighted + Active: the initial paint is a no-op Restore (last is
    // initialised to Restore). Disable -> Tint(disabled): apply (1 read + 1
    // write). Re-enable -> Restore: write the base palette back (1 read + 1
    // write). Total: 2 reads, 2 writes.
    let mut mock_api = MockWindowsApi::new();
    expect_palette_ops(&mut mock_api, sample_palette(), 2, 2);

    let (state_sender, state_receiver) = watch::channel(ClientState::Active);
    let (highlight_sender, highlight_receiver) = watch::channel(false);

    let visuals = run_visuals_loop(
        &mock_api,
        state_receiver,
        highlight_receiver,
        Some(palette_snapshot(sample_palette())),
        disabled,
        highlighted,
    );
    let driver = async {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        state_sender.send_replace(ClientState::Disabled);
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        state_sender.send_replace(ClientState::Active);
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        drop(state_sender);
        drop(highlight_sender);
    };

    tokio::join!(visuals, driver);
}
