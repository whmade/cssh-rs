mod daemon_test {
    use std::{
        ffi::{c_void, OsString},
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc, Mutex,
        },
    };

    use tokio::{
        sync::{broadcast, watch},
        task::JoinHandle,
    };
    use windows::Win32::Foundation::{HANDLE, HWND};
    use windows::Win32::System::Console::{
        CAPSLOCK_ON, ENHANCED_KEY, KEY_EVENT_RECORD, KEY_EVENT_RECORD_0, LEFT_ALT_PRESSED,
        LEFT_CTRL_PRESSED, NUMLOCK_ON, RIGHT_ALT_PRESSED, RIGHT_CTRL_PRESSED, SCROLLLOCK_ON,
        SHIFT_PRESSED,
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        VIRTUAL_KEY, VK_C, VK_D, VK_DOWN, VK_E, VK_H, VK_J, VK_K, VK_L, VK_LEFT, VK_N, VK_R,
        VK_RIGHT, VK_T, VK_UP, VK_X,
    };

    use cssh_rs_platform::ControlChannelClient;
    use cssh_rs_protocol::v1::input::{ClientRunState, InputEvent};
    use cssh_rs_protocol::v1::keycode::{KeyCode, Modifiers};
    use cssh_rs_protocol::v1::limits::DEFAULT_MAX_FRAME_LEN;
    use cssh_rs_protocol::v1::message::DaemonToClient;
    use cssh_rs_protocol::ClientState;

    use crate::{
        client::{client_handshake, read_message},
        daemon::{
            build_client_args, classify_control_mode_key, classify_enable_disable_submenu_key,
            encode_broadcast, expand_hosts,
            grid::{grid_dimensions, ClientGrid},
            named_pipe_server_routine, next_submenu_selection, resolve_cluster_tags,
            workspace::WorkspaceArea,
            Client, ClientBroadcast, Clients, ControlModeAction, ControlModeState, Daemon,
            EnableDisableSubmenuAction, HWNDWrapper, NavigationDirection,
        },
        utils::{
            config::{Cluster, DaemonConfig, EdgeBehavior},
            windows::{WindowsControlChannelClient, WindowsControlChannelServer},
        },
    };

    /// Stable 16:9 workspace fixture used by the submenu dispatch tests so
    /// `grid_dimensions` is deterministic regardless of host monitor size.
    fn test_workspace_area() -> WorkspaceArea {
        return WorkspaceArea {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
            x_fixed_frame: 0,
            y_fixed_frame: 0,
            x_size_frame: 0,
            y_size_frame: 0,
        };
    }

    #[test]
    fn test_build_client_args_with_all_options() {
        let args = build_client_args(
            "alice@host",
            Some("ignored".to_string()),
            Some(2222),
            true,
            "pipe",
        );
        assert_eq!(
            args,
            [
                "-d",
                "-u",
                "alice",
                "-p",
                "2222",
                "client",
                "--daemon-channel",
                "pipe",
                "--",
                "host",
            ]
        );
    }

    #[test]
    fn test_hwnd_wrapper_equality() {
        assert_eq!(
            HWNDWrapper {
                hwdn: HWND(std::ptr::dangling_mut::<c_void>())
            },
            HWNDWrapper {
                hwdn: HWND(std::ptr::dangling_mut::<c_void>())
            }
        );
        assert_ne!(
            HWNDWrapper {
                hwdn: HWND(std::ptr::dangling_mut::<c_void>())
            },
            HWNDWrapper {
                hwdn: HWND(unsafe { std::ptr::dangling_mut::<c_void>().add(1) })
            }
        );
    }

    #[test]
    fn test_resolve_cluster_tags() {
        let hosts: Vec<&str> = vec!["host0", "cluster1", "host3", "host0", "host1"];
        let clusters: Vec<Cluster> = vec![Cluster {
            name: "cluster1".to_string(),
            hosts: vec!["host1".to_string(), "host2".to_string()],
        }];
        assert_eq!(
            resolve_cluster_tags(hosts, &clusters),
            vec!["host0", "host1", "host2", "host3", "host0", "host1"]
        );
    }

    #[test]
    fn test_resolve_cluster_tags_no_cluster() {
        let hosts: Vec<&str> = vec!["host0"];
        let clusters: Vec<Cluster> = vec![Cluster {
            name: "cluster1".to_string(),
            hosts: vec!["host1".to_string(), "host2".to_string()],
        }];
        assert_eq!(resolve_cluster_tags(hosts, &clusters), vec!["host0"]);
    }

    #[test]
    fn test_resolve_cluster_tags_simple_nested_cluster() {
        let hosts: Vec<&str> = vec!["cluster2"];
        let clusters: Vec<Cluster> = vec![
            Cluster {
                name: "cluster1".to_string(),
                hosts: vec!["host1".to_string(), "host2".to_string()],
            },
            Cluster {
                name: "cluster2".to_string(),
                hosts: vec!["cluster1".to_owned(), "host3".to_owned()],
            },
        ];
        assert_eq!(
            resolve_cluster_tags(hosts, &clusters),
            vec!["host1", "host2", "host3"]
        );
    }

    #[test]
    fn test_expand_hosts_brace_only() {
        let hosts: Vec<&str> = vec!["host{1..3}.local"];
        let clusters: Vec<Cluster> = vec![];
        assert_eq!(
            expand_hosts(hosts, &clusters),
            vec!["host1.local", "host2.local", "host3.local"]
        );
    }

    #[test]
    fn test_expand_hosts_cluster_with_brace_member() {
        let hosts: Vec<&str> = vec!["clusterA"];
        let clusters: Vec<Cluster> = vec![Cluster {
            name: "clusterA".to_string(),
            hosts: vec!["box{1..2}.local".to_string()],
        }];
        assert_eq!(
            expand_hosts(hosts, &clusters),
            vec!["box1.local", "box2.local"]
        );
    }

    #[test]
    fn test_expand_hosts_mixed_cluster_tag_and_brace() {
        let hosts: Vec<&str> = vec!["clusterA", "edge{1..2}.local"];
        let clusters: Vec<Cluster> = vec![Cluster {
            name: "clusterA".to_string(),
            hosts: vec!["a".to_string(), "b".to_string()],
        }];
        assert_eq!(
            expand_hosts(hosts, &clusters),
            vec!["a", "b", "edge1.local", "edge2.local"]
        );
    }

    #[test]
    fn test_expand_hosts_plain_hostnames_unchanged() {
        let hosts: Vec<&str> = vec!["a.local", "b.local"];
        let clusters: Vec<Cluster> = vec![];
        assert_eq!(expand_hosts(hosts, &clusters), vec!["a.local", "b.local"]);
    }

    fn unique_pipe_name() -> OsString {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        return OsString::from(format!(
            r"\\.\pipe\cssh-rs-daemon-test-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
    }

    /// Run the routine for one client registered as `registered_pid` and
    /// connect to it.
    async fn connect_to_routine(
        registered_pid: u32,
        state_sender: &watch::Sender<ClientState>,
        broadcast_sender: &broadcast::Sender<ClientBroadcast>,
    ) -> (WindowsControlChannelClient, JoinHandle<()>) {
        let endpoint = unique_pipe_name();
        let server = WindowsControlChannelServer::bind(&endpoint).expect("bind");
        let mut clients = Clients::new();
        clients.push(Client {
            hostname: "test-host".to_string(),
            window_handle: HWND(std::ptr::null_mut()),
            process_handle: HANDLE::default(),
            process_id: registered_pid,
            state_sender: state_sender.clone(),
            highlight_sender: watch::channel(false).0,
            tile_index: 0,
        });
        let mut receiver = broadcast_sender.subscribe();
        let routine = tokio::spawn(async move {
            named_pipe_server_routine(server, &mut receiver, Arc::new(Mutex::new(clients))).await;
        });
        let mut client = WindowsControlChannelClient::new();
        client.connect(&endpoint).await.expect("connect");
        return (client, routine);
    }

    async fn next_non_keep_alive(
        client: &mut WindowsControlChannelClient,
        pending: &mut Vec<u8>,
    ) -> DaemonToClient {
        loop {
            match read_message(client, pending, DEFAULT_MAX_FRAME_LEN)
                .await
                .expect("daemon message")
            {
                DaemonToClient::KeepAlive => {}
                message => return message,
            }
        }
    }

    fn keystroke(text: &str) -> DaemonToClient {
        return DaemonToClient::Input {
            event: InputEvent::Key {
                code: KeyCode::Char(text.to_string()),
                modifiers: Modifiers::NONE,
                text: Some(text.to_string()),
            },
        };
    }

    fn send_keystroke(broadcast_sender: &broadcast::Sender<ClientBroadcast>, text: &str) {
        broadcast_sender
            .send(ClientBroadcast::Keystroke(encode_broadcast(&keystroke(
                text,
            ))))
            .expect("broadcast");
    }

    #[tokio::test]
    async fn test_named_pipe_server_routine_pushes_state_set_before_connect() {
        let state_sender = watch::channel(ClientState::Disabled).0;
        let (broadcast_sender, _) = broadcast::channel(16);
        let (mut client, _routine) =
            connect_to_routine(std::process::id(), &state_sender, &broadcast_sender).await;

        let (_, mut pending) = client_handshake(&mut client).await;

        assert_eq!(
            next_non_keep_alive(&mut client, &mut pending).await,
            DaemonToClient::StateChange {
                state: ClientRunState::Disabled
            }
        );
        assert_eq!(
            next_non_keep_alive(&mut client, &mut pending).await,
            DaemonToClient::Highlight { on: false }
        );
    }

    #[tokio::test]
    async fn test_named_pipe_server_routine_gates_keystrokes_but_not_terminate() {
        let state_sender = watch::channel(ClientState::Active).0;
        let (broadcast_sender, _) = broadcast::channel(16);
        let (mut client, routine) =
            connect_to_routine(std::process::id(), &state_sender, &broadcast_sender).await;
        let (_, mut pending) = client_handshake(&mut client).await;
        next_non_keep_alive(&mut client, &mut pending).await;
        next_non_keep_alive(&mut client, &mut pending).await;

        send_keystroke(&broadcast_sender, "a");
        assert_eq!(
            next_non_keep_alive(&mut client, &mut pending).await,
            keystroke("a")
        );

        state_sender.send_replace(ClientState::Disabled);
        assert_eq!(
            next_non_keep_alive(&mut client, &mut pending).await,
            DaemonToClient::StateChange {
                state: ClientRunState::Disabled
            }
        );

        let terminate = DaemonToClient::Terminate {
            reason: "test".to_string(),
        };
        send_keystroke(&broadcast_sender, "b");
        broadcast_sender
            .send(ClientBroadcast::Terminate(encode_broadcast(&terminate)))
            .expect("broadcast");
        assert_eq!(
            next_non_keep_alive(&mut client, &mut pending).await,
            terminate
        );
        routine.await.expect("routine ends after Terminate");
    }

    #[tokio::test]
    async fn test_named_pipe_server_routine_unknown_pid_panics() {
        let state_sender = watch::channel(ClientState::Active).0;
        let (broadcast_sender, _) = broadcast::channel(16);
        let (mut client, routine) = connect_to_routine(0, &state_sender, &broadcast_sender).await;

        client_handshake(&mut client).await;

        assert!(routine.await.unwrap_err().is_panic());
    }

    #[tokio::test]
    async fn test_named_pipe_server_routine_lagged() {
        let state_sender = watch::channel(ClientState::Disabled).0;
        let (broadcast_sender, _) = broadcast::channel(2);
        let (mut client, routine) =
            connect_to_routine(std::process::id(), &state_sender, &broadcast_sender).await;
        let (_, mut pending) = client_handshake(&mut client).await;

        for _ in 0..16 {
            send_keystroke(&broadcast_sender, "a");
        }

        loop {
            match read_message(&mut client, &mut pending, DEFAULT_MAX_FRAME_LEN)
                .await
                .expect("routine survives the lag")
            {
                DaemonToClient::KeepAlive => break,
                message => assert!(!matches!(message, DaemonToClient::Input { .. })),
            }
        }
        drop(client);
        routine.await.expect("routine ends without panicking");
    }

    #[test]
    #[should_panic(expected = "Duplicate client PID")]
    fn test_clients_push_duplicate_pid_panics() {
        let mut clients = Clients::new();
        let make_client = |pid: u32| {
            return Client {
                hostname: "host".to_owned(),
                window_handle: HWND(std::ptr::null_mut()),
                process_handle: HANDLE::default(),
                process_id: pid,
                state_sender: watch::channel(ClientState::Active).0,
                highlight_sender: watch::channel(false).0,
                tile_index: 0,
            };
        };
        clients.push(make_client(1000));
        clients.push(make_client(1000)); // duplicate - must panic
    }

    #[test]
    fn test_clients_push_and_lookup() {
        let mut clients = Clients::new();
        assert!(clients.is_empty());
        assert_eq!(clients.len(), 0);

        let client_a = Client {
            hostname: "host-a".to_owned(),
            window_handle: HWND(std::ptr::null_mut()),
            process_handle: HANDLE::default(),
            process_id: 1000,
            state_sender: watch::channel(ClientState::Active).0,
            highlight_sender: watch::channel(false).0,
            tile_index: 0,
        };
        let client_b = Client {
            hostname: "host-b".to_owned(),
            window_handle: HWND(std::ptr::null_mut()),
            process_handle: HANDLE::default(),
            process_id: 2000,
            state_sender: watch::channel(ClientState::Active).0,
            highlight_sender: watch::channel(false).0,
            tile_index: 0,
        };
        let client_c = Client {
            hostname: "host-c".to_owned(),
            window_handle: HWND(std::ptr::null_mut()),
            process_handle: HANDLE::default(),
            process_id: 3000,
            state_sender: watch::channel(ClientState::Active).0,
            highlight_sender: watch::channel(false).0,
            tile_index: 0,
        };

        clients.push(client_a);
        clients.push(client_b);
        clients.push(client_c);

        assert_eq!(clients.len(), 3);
        assert!(!clients.is_empty());
        assert_eq!(clients.get_by_pid(1000).unwrap().hostname, "host-a");
        assert_eq!(clients.get_by_pid(2000).unwrap().hostname, "host-b");
        assert_eq!(clients.get_by_pid(3000).unwrap().hostname, "host-c");
        assert!(clients.get_by_pid(9999).is_none());

        // iter preserves insertion order
        let hostnames: Vec<&str> = clients.iter().map(|c| return c.hostname.as_str()).collect();
        assert_eq!(hostnames, vec!["host-a", "host-b", "host-c"]);

        // retain rebuilds the PID index so lookups remain consistent
        clients.retain(|client| return client.process_id != 2000);
        assert_eq!(clients.len(), 2);
        assert!(clients.get_by_pid(2000).is_none());
        assert_eq!(clients.get_by_pid(1000).unwrap().hostname, "host-a");
        assert_eq!(clients.get_by_pid(3000).unwrap().hostname, "host-c");
        let hostnames_after_retain: Vec<&str> =
            clients.iter().map(|c| return c.hostname.as_str()).collect();
        assert_eq!(hostnames_after_retain, vec!["host-a", "host-c"]);
    }

    /// Builds a [`Client`] with the given PID and initial [`ClientState`].
    fn make_client_with_state(pid: u32, state: ClientState) -> Client {
        return Client {
            hostname: format!("host-{pid}"),
            window_handle: HWND(std::ptr::null_mut()),
            process_handle: HANDLE::default(),
            process_id: pid,
            state_sender: watch::channel(state).0,
            highlight_sender: watch::channel(false).0,
            // Overwritten by `Clients::push` to its dense list-position.
            tile_index: 0,
        };
    }

    /// Verifies that the `[t]oggle enabled` control-mode handler flips each
    /// client's [`ClientState`] independently and is its own inverse over
    /// two invocations.
    ///
    /// Mirrors the snapshot-then-flip logic in
    /// [`crate::daemon::Daemon::handle_control_mode`]'s `VK_T` arm so the
    /// per-client toggle behaviour is exercised without standing up a full
    /// [`crate::daemon::Daemon`].
    #[test]
    fn test_toggle_flips_each_client_independently() {
        let mut clients = Clients::new();
        clients.push(make_client_with_state(1, ClientState::Active));
        clients.push(make_client_with_state(2, ClientState::Disabled));
        clients.push(make_client_with_state(3, ClientState::Active));
        clients.push(make_client_with_state(4, ClientState::Disabled));

        let snapshot = |c: &Clients| -> Vec<ClientState> {
            return c
                .iter()
                .map(|client| return *client.state_sender.borrow())
                .collect();
        };

        let initial = snapshot(&clients);
        assert_eq!(
            initial,
            vec![
                ClientState::Active,
                ClientState::Disabled,
                ClientState::Active,
                ClientState::Disabled,
            ]
        );

        // Press `t` once: snapshot every state, then flip each.
        let toggle = |c: &Clients| {
            let flips: Vec<ClientState> = c
                .iter()
                .map(|client| {
                    return match *client.state_sender.borrow() {
                        ClientState::Active => ClientState::Disabled,
                        ClientState::Disabled => ClientState::Active,
                    };
                })
                .collect();
            // `send_replace` succeeds even when no task has subscribed;
            // tests don't spin up the pipe-server routine that would
            // normally hold the receiver.
            for (client, flipped) in c.iter().zip(flips) {
                client.state_sender.send_replace(flipped);
            }
        };

        toggle(&clients);
        assert_eq!(
            snapshot(&clients),
            vec![
                ClientState::Disabled,
                ClientState::Active,
                ClientState::Disabled,
                ClientState::Active,
            ]
        );

        // Press `t` again: every client flips back to its initial state.
        toggle(&clients);
        assert_eq!(snapshot(&clients), initial);
    }

    /// Collects every client's [`ClientState`] in insertion order.
    fn snapshot_states(clients: &Clients) -> Vec<ClientState> {
        return clients
            .iter()
            .map(|client| return *client.state_sender.borrow())
            .collect();
    }

    /// Builds a [`KEY_EVENT_RECORD`] for a key-down press with no
    /// active modifier bits, mirroring the matcher used by the
    /// submenu's `[e]`/`[d]`/`[t]` arms.
    fn submenu_key_event(virtual_key: VIRTUAL_KEY) -> KEY_EVENT_RECORD {
        return submenu_key_event_with_state(virtual_key, 0);
    }

    /// Same as [`submenu_key_event`] but with a caller-supplied
    /// `dwControlKeyState`. Used by the GH #196 regression to drive
    /// the submenu with lock-state bits engaged.
    fn submenu_key_event_with_state(
        virtual_key: VIRTUAL_KEY,
        control_key_state: u32,
    ) -> KEY_EVENT_RECORD {
        return KEY_EVENT_RECORD {
            bKeyDown: true.into(),
            wRepeatCount: 1,
            wVirtualKeyCode: virtual_key.0,
            wVirtualScanCode: 0,
            uChar: KEY_EVENT_RECORD_0 { UnicodeChar: 0 },
            dwControlKeyState: control_key_state,
        };
    }

    /// Builds a fresh [`MockWindowsApi`] with no expectations set.
    ///
    /// Suitable for submenu dispatch tests that exercise only the
    /// `[e]`/`[d]`/`[t]` arms (which never touch the Windows API).
    /// Tests that drive the `Navigate` arm must additionally stub the
    /// console calls performed by [`crate::utils::windows::clear_screen`]
    /// (see `mock_with_clear_screen`).
    fn mock_no_calls() -> crate::utils::windows::MockWindowsApi {
        return crate::utils::windows::MockWindowsApi::new();
    }

    /// Builds a [`MockWindowsApi`] that satisfies the console calls
    /// [`crate::utils::windows::clear_screen`] performs when the
    /// submenu renderer redraws after a navigation keystroke.
    ///
    /// Mirrors the mock setup used by
    /// `test_esc_in_active_state_is_consumed_and_resets_to_inactive`.
    fn mock_with_clear_screen() -> crate::utils::windows::MockWindowsApi {
        use windows::Win32::System::Console::{CONSOLE_SCREEN_BUFFER_INFO, COORD};
        let mut mock = crate::utils::windows::MockWindowsApi::new();
        mock.expect_get_console_screen_buffer_info().returning(|| {
            return Ok(CONSOLE_SCREEN_BUFFER_INFO {
                dwSize: COORD { X: 80, Y: 25 },
                ..Default::default()
            });
        });
        mock.expect_scroll_console_screen_buffer()
            .returning(|_, _, _| return Ok(()));
        mock.expect_set_console_cursor_position()
            .returning(|_| return Ok(()));
        return mock;
    }

    /// Verifies that `VK_E` in the enable/disable submenu enables
    /// only the currently selected client (index 0 here) and keeps
    /// the submenu open so the user can chain further
    /// enable/disable/toggle actions across clients without
    /// re-entering the submenu.
    #[test]
    fn test_submenu_e_enables_only_selected_client_and_stays_open() {
        let mut clients = Clients::new();
        clients.push(make_client_with_state(1, ClientState::Disabled));
        clients.push(make_client_with_state(2, ClientState::Disabled));
        clients.push(make_client_with_state(3, ClientState::Disabled));
        let clients = Mutex::new(clients);

        let config = DaemonConfig::default();
        let clusters: Vec<Cluster> = Vec::new();
        let mut daemon = Daemon::for_test(
            &config,
            &clusters,
            ControlModeState::EnableDisableSubmenu {
                highlighted_pid: Some(1),
                anchor_col: Some(0),
            },
        );

        daemon.handle_enable_disable_submenu_key(
            &mock_no_calls(),
            &clients,
            &test_workspace_area(),
            submenu_key_event(VK_E),
        );

        assert_eq!(
            snapshot_states(&clients.lock().unwrap()),
            vec![
                ClientState::Active,
                ClientState::Disabled,
                ClientState::Disabled,
            ]
        );
        assert!(matches!(
            daemon.control_mode_state,
            ControlModeState::EnableDisableSubmenu { .. }
        ));
    }

    /// Verifies that `VK_D` in the enable/disable submenu disables
    /// only the currently selected client (index 0 here) and keeps
    /// the submenu open so the user can chain further
    /// enable/disable/toggle actions across clients without
    /// re-entering the submenu.
    #[test]
    fn test_submenu_d_disables_only_selected_client_and_stays_open() {
        let mut clients = Clients::new();
        clients.push(make_client_with_state(1, ClientState::Active));
        clients.push(make_client_with_state(2, ClientState::Active));
        clients.push(make_client_with_state(3, ClientState::Active));
        let clients = Mutex::new(clients);

        let config = DaemonConfig::default();
        let clusters: Vec<Cluster> = Vec::new();
        let mut daemon = Daemon::for_test(
            &config,
            &clusters,
            ControlModeState::EnableDisableSubmenu {
                highlighted_pid: Some(1),
                anchor_col: Some(0),
            },
        );

        daemon.handle_enable_disable_submenu_key(
            &mock_no_calls(),
            &clients,
            &test_workspace_area(),
            submenu_key_event(VK_D),
        );

        assert_eq!(
            snapshot_states(&clients.lock().unwrap()),
            vec![
                ClientState::Disabled,
                ClientState::Active,
                ClientState::Active,
            ]
        );
        assert!(matches!(
            daemon.control_mode_state,
            ControlModeState::EnableDisableSubmenu { .. }
        ));
    }

    /// Verifies that `VK_T` in the enable/disable submenu flips only
    /// the currently selected client's state, keeps the submenu open
    /// after the flip, and is its own inverse over two consecutive
    /// presses without needing to re-enter the submenu in between.
    #[test]
    fn test_submenu_t_toggles_only_selected_client_and_stays_open() {
        let mut clients = Clients::new();
        clients.push(make_client_with_state(1, ClientState::Active));
        clients.push(make_client_with_state(2, ClientState::Disabled));
        clients.push(make_client_with_state(3, ClientState::Active));
        let clients = Mutex::new(clients);

        let initial = snapshot_states(&clients.lock().unwrap());

        let config = DaemonConfig::default();
        let clusters: Vec<Cluster> = Vec::new();
        let mut daemon = Daemon::for_test(
            &config,
            &clusters,
            ControlModeState::EnableDisableSubmenu {
                highlighted_pid: Some(1),
                anchor_col: Some(0),
            },
        );

        daemon.handle_enable_disable_submenu_key(
            &mock_no_calls(),
            &clients,
            &test_workspace_area(),
            submenu_key_event(VK_T),
        );
        assert_eq!(
            snapshot_states(&clients.lock().unwrap()),
            vec![
                ClientState::Disabled,
                ClientState::Disabled,
                ClientState::Active,
            ]
        );
        assert!(matches!(
            daemon.control_mode_state,
            ControlModeState::EnableDisableSubmenu { .. }
        ));

        // Second press without re-entering the submenu: the submenu
        // must still be open from the first press, and the toggle is
        // its own inverse.
        daemon.handle_enable_disable_submenu_key(
            &mock_no_calls(),
            &clients,
            &test_workspace_area(),
            submenu_key_event(VK_T),
        );
        assert_eq!(snapshot_states(&clients.lock().unwrap()), initial);
        assert!(matches!(
            daemon.control_mode_state,
            ControlModeState::EnableDisableSubmenu { .. }
        ));
    }

    /// Verifies that an unrecognised key in the enable/disable
    /// submenu leaves every client state unchanged and keeps the
    /// submenu open for the next press.
    #[test]
    fn test_submenu_ignores_unmapped_key() {
        let mut clients = Clients::new();
        clients.push(make_client_with_state(1, ClientState::Active));
        clients.push(make_client_with_state(2, ClientState::Disabled));
        let clients = Mutex::new(clients);

        let initial = snapshot_states(&clients.lock().unwrap());

        let config = DaemonConfig::default();
        let clusters: Vec<Cluster> = Vec::new();
        let mut daemon = Daemon::for_test(
            &config,
            &clusters,
            ControlModeState::EnableDisableSubmenu {
                highlighted_pid: Some(1),
                anchor_col: Some(0),
            },
        );

        daemon.handle_enable_disable_submenu_key(
            &mock_no_calls(),
            &clients,
            &test_workspace_area(),
            submenu_key_event(VK_X),
        );

        assert_eq!(snapshot_states(&clients.lock().unwrap()), initial);
        assert!(matches!(
            daemon.control_mode_state,
            ControlModeState::EnableDisableSubmenu { .. }
        ));
    }

    /// Verifies that pressing `VK_E` with no clients tracked is a
    /// no-op for the client list (and does not panic) while leaving
    /// the submenu open for the next press.
    #[test]
    fn test_submenu_no_panic_with_empty_clients() {
        let clients = Mutex::new(Clients::new());

        let config = DaemonConfig::default();
        let clusters: Vec<Cluster> = Vec::new();
        // Submenu entry on an empty cluster leaves the highlight at
        // `None`; the dispatch must not panic.
        let mut daemon = Daemon::for_test(
            &config,
            &clusters,
            ControlModeState::EnableDisableSubmenu {
                highlighted_pid: None,
                anchor_col: None,
            },
        );

        daemon.handle_enable_disable_submenu_key(
            &mock_no_calls(),
            &clients,
            &test_workspace_area(),
            submenu_key_event(VK_E),
        );

        assert!(clients.lock().unwrap().iter().next().is_none());
        assert!(matches!(
            daemon.control_mode_state,
            ControlModeState::EnableDisableSubmenu { .. }
        ));
    }

    /// Regression for GH #196: control-mode dispatch must ignore
    /// lock toggles (`CAPSLOCK_ON`, `NUMLOCK_ON`, `SCROLLLOCK_ON`)
    /// and the `ENHANCED_KEY` flag when matching `(VK_*, 0)` arms.
    /// Those bits live in `dwControlKeyState` alongside the real
    /// modifier bits (Ctrl/Alt/Shift), and an enabled CapsLock
    /// previously made the entire field non-zero, silently skipping
    /// every action.
    ///
    /// Conversely, any real modifier bit must survive the masking
    /// so combos like Shift+R do not collapse into the plain-R arm.
    #[test]
    fn test_control_mode_classifiers_ignore_lock_state_and_enhanced_key() {
        let benign_states = [
            0,
            CAPSLOCK_ON,
            NUMLOCK_ON,
            SCROLLLOCK_ON,
            ENHANCED_KEY,
            CAPSLOCK_ON | NUMLOCK_ON | SCROLLLOCK_ON | ENHANCED_KEY,
        ];
        let main_expected = [
            (VK_R, ControlModeAction::Retile),
            (VK_E, ControlModeAction::OpenEnableDisableSubmenu),
            (VK_T, ControlModeAction::ToggleEnabled),
            (VK_N, ControlModeAction::EnableAll),
            (VK_C, ControlModeAction::CreateWindows),
            (VK_H, ControlModeAction::CopyHostnames),
        ];
        let submenu_expected = [
            (VK_E, EnableDisableSubmenuAction::Enable),
            (VK_D, EnableDisableSubmenuAction::Disable),
            (VK_T, EnableDisableSubmenuAction::Toggle),
            (
                VK_UP,
                EnableDisableSubmenuAction::Navigate(NavigationDirection::Up),
            ),
            (
                VK_K,
                EnableDisableSubmenuAction::Navigate(NavigationDirection::Up),
            ),
            (
                VK_DOWN,
                EnableDisableSubmenuAction::Navigate(NavigationDirection::Down),
            ),
            (
                VK_J,
                EnableDisableSubmenuAction::Navigate(NavigationDirection::Down),
            ),
            (
                VK_LEFT,
                EnableDisableSubmenuAction::Navigate(NavigationDirection::Left),
            ),
            (
                VK_H,
                EnableDisableSubmenuAction::Navigate(NavigationDirection::Left),
            ),
            (
                VK_RIGHT,
                EnableDisableSubmenuAction::Navigate(NavigationDirection::Right),
            ),
            (
                VK_L,
                EnableDisableSubmenuAction::Navigate(NavigationDirection::Right),
            ),
        ];

        for state in benign_states {
            for (vk, action) in &main_expected {
                assert_eq!(
                    &classify_control_mode_key(*vk, state),
                    action,
                    "main menu: VK {vk:?} with state 0x{state:08X} must classify as {action:?}",
                );
            }
            for (vk, action) in &submenu_expected {
                assert_eq!(
                    &classify_enable_disable_submenu_key(*vk, state),
                    action,
                    "submenu: VK {vk:?} with state 0x{state:08X} must classify as {action:?}",
                );
            }
        }

        let modifier_states = [
            LEFT_CTRL_PRESSED,
            RIGHT_CTRL_PRESSED,
            LEFT_ALT_PRESSED,
            RIGHT_ALT_PRESSED,
            SHIFT_PRESSED,
            LEFT_CTRL_PRESSED | CAPSLOCK_ON,
            SHIFT_PRESSED | NUMLOCK_ON | ENHANCED_KEY,
        ];
        for state in modifier_states {
            for (vk, _) in &main_expected {
                assert_eq!(
                    classify_control_mode_key(*vk, state),
                    ControlModeAction::NoOp,
                    "main menu: VK {vk:?} with modifier state 0x{state:08X} must NOT fire the plain-key arm",
                );
            }
            for (vk, _) in &submenu_expected {
                assert_eq!(
                    classify_enable_disable_submenu_key(*vk, state),
                    EnableDisableSubmenuAction::NoOp,
                    "submenu: VK {vk:?} with modifier state 0x{state:08X} must NOT fire the plain-key arm",
                );
            }
        }
    }

    /// End-to-end regression for GH #196 at the dispatch level: when
    /// CapsLock (or any other lock toggle) is engaged, pressing
    /// `[e]` in the enable/disable submenu must still enable the
    /// first client. Before the [`MODIFIER_MASK`][1] fix, the
    /// non-zero `dwControlKeyState` would skip the `(VK_E, 0)` arm
    /// and the press would silently do nothing.
    ///
    /// [1]: crate::daemon
    #[test]
    fn test_submenu_dispatch_ignores_lock_state_bits() {
        let mut clients = Clients::new();
        clients.push(make_client_with_state(1, ClientState::Disabled));
        clients.push(make_client_with_state(2, ClientState::Disabled));
        let clients = Mutex::new(clients);

        let config = DaemonConfig::default();
        let clusters: Vec<Cluster> = Vec::new();
        let mut daemon = Daemon::for_test(
            &config,
            &clusters,
            ControlModeState::EnableDisableSubmenu {
                highlighted_pid: Some(1),
                anchor_col: Some(0),
            },
        );

        daemon.handle_enable_disable_submenu_key(
            &mock_no_calls(),
            &clients,
            &test_workspace_area(),
            submenu_key_event_with_state(VK_E, CAPSLOCK_ON | NUMLOCK_ON | ENHANCED_KEY),
        );

        assert_eq!(
            snapshot_states(&clients.lock().unwrap()),
            vec![ClientState::Active, ClientState::Disabled],
            "VK_E with lock-state bits set must still enable the selected client",
        );
        assert!(matches!(
            daemon.control_mode_state,
            ControlModeState::EnableDisableSubmenu { .. }
        ));
    }

    /// Regression test for #197: when control mode is `Active` and the
    /// user presses `Esc`, `control_mode_is_active` must report that the
    /// keystroke was consumed (`true`) so that `handle_input_record`
    /// suppresses the broadcast. Before the fix it returned `false`,
    /// which leaked the `Esc` to every connected client.
    #[test]
    fn test_esc_in_active_state_is_consumed_and_resets_to_inactive() {
        use crate::utils::windows::MockWindowsApi;
        use windows::core::BOOL;
        use windows::Win32::System::Console::{CONSOLE_SCREEN_BUFFER_INFO, COORD, INPUT_RECORD_0};
        use windows::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;

        // Arrange: a daemon already in `Active` control mode and a mock
        // that stubs the console calls `quit_control_mode` makes via
        // `print_instructions` -> `clear_screen`.
        let config = DaemonConfig::default();
        let clusters: Vec<Cluster> = Vec::new();
        let mut daemon = Daemon::for_test(&config, &clusters, ControlModeState::Active);

        let mut mock = MockWindowsApi::new();
        mock.expect_get_console_screen_buffer_info().returning(|| {
            return Ok(CONSOLE_SCREEN_BUFFER_INFO {
                dwSize: COORD { X: 80, Y: 25 },
                ..Default::default()
            });
        });
        mock.expect_scroll_console_screen_buffer()
            .returning(|_, _, _| return Ok(()));
        mock.expect_set_console_cursor_position()
            .returning(|_| return Ok(()));

        let esc_input = INPUT_RECORD_0 {
            KeyEvent: KEY_EVENT_RECORD {
                bKeyDown: BOOL(1),
                wRepeatCount: 1,
                wVirtualKeyCode: VK_ESCAPE.0,
                wVirtualScanCode: 0,
                uChar: KEY_EVENT_RECORD_0 { UnicodeChar: 0 },
                dwControlKeyState: 0,
            },
        };

        // Act
        let clients: Arc<Mutex<Clients>> = Arc::new(Mutex::new(Clients::new()));
        let consumed = daemon.control_mode_is_active(&mock, &clients, esc_input);

        // Assert: the `Esc` is reported as owned by control mode (so the
        // caller will skip forwarding it) and the state machine is back
        // to `Inactive`.
        assert!(consumed);
        assert_eq!(daemon.control_mode_state, ControlModeState::Inactive);
    }

    /// Verifies that `quit_control_mode` transitions back to
    /// `Inactive` so any submenu highlight state carried on the
    /// previous variant is dropped along with it.
    #[test]
    fn test_quit_control_mode_transitions_to_inactive() {
        use crate::utils::windows::MockWindowsApi;
        let config = DaemonConfig::default();
        let clusters: Vec<Cluster> = Vec::new();
        let mut daemon = Daemon::for_test(
            &config,
            &clusters,
            ControlModeState::EnableDisableSubmenu {
                highlighted_pid: Some(3),
                anchor_col: Some(0),
            },
        );

        let mut mock = MockWindowsApi::new();
        mock.expect_get_console_screen_buffer_info().returning(|| {
            use windows::Win32::System::Console::{CONSOLE_SCREEN_BUFFER_INFO, COORD};
            return Ok(CONSOLE_SCREEN_BUFFER_INFO {
                dwSize: COORD { X: 80, Y: 25 },
                ..Default::default()
            });
        });
        mock.expect_scroll_console_screen_buffer()
            .returning(|_, _, _| return Ok(()));
        mock.expect_set_console_cursor_position()
            .returning(|_| return Ok(()));

        daemon.quit_control_mode(&mock);

        assert_eq!(daemon.control_mode_state, ControlModeState::Inactive);
    }

    /// Returns the dense 3x2 grid `[1,2,3 / 4,5,6]` used by the basic
    /// horizontal/vertical step tests.
    fn dense_3x2_grid() -> ClientGrid {
        return ClientGrid::from_tiled_pids(
            &[(1, 0), (2, 1), (3, 2), (4, 3), (5, 4), (6, 5)],
            6,
            3,
            2,
        );
    }

    /// Verifies horizontal stepping (Left/Right) clamps at the row edges
    /// when `EdgeBehavior::Clamp` is set.
    #[test]
    fn test_next_submenu_selection_horizontal_clamp() {
        let grid = dense_3x2_grid();
        assert_eq!(
            next_submenu_selection(
                &grid,
                Some(1),
                Some(0),
                NavigationDirection::Right,
                EdgeBehavior::Clamp,
            ),
            (Some(2), Some(1)),
        );
        assert_eq!(
            next_submenu_selection(
                &grid,
                Some(3),
                Some(2),
                NavigationDirection::Right,
                EdgeBehavior::Clamp,
            ),
            (Some(3), Some(2)),
            "Right at the right edge clamps in place",
        );
        assert_eq!(
            next_submenu_selection(
                &grid,
                Some(1),
                Some(0),
                NavigationDirection::Left,
                EdgeBehavior::Clamp,
            ),
            (Some(1), Some(0)),
            "Left at the left edge clamps in place",
        );
    }

    /// Regression: a clamped horizontal no-op must preserve the
    /// in-flight anchor column. A prior vertical snap into a gap can
    /// leave `anchor_col` pointing at a different column than the
    /// current cell; re-aligning the anchor to the current cell on a
    /// clamped Left/Right keypress would silently break the "anchor
    /// carried across vertical moves" invariant the next Up/Down
    /// relies on.
    #[test]
    fn test_next_submenu_selection_horizontal_clamp_preserves_anchor() {
        let grid = dense_3x2_grid();
        assert_eq!(
            next_submenu_selection(
                &grid,
                Some(1),
                Some(2),
                NavigationDirection::Left,
                EdgeBehavior::Clamp,
            ),
            (Some(1), Some(2)),
            "Left clamp keeps the stale anchor, not the cell's own column",
        );
        assert_eq!(
            next_submenu_selection(
                &grid,
                Some(3),
                Some(0),
                NavigationDirection::Right,
                EdgeBehavior::Clamp,
            ),
            (Some(3), Some(0)),
            "Right clamp keeps the stale anchor, not the cell's own column",
        );
    }

    /// Verifies vertical stepping (Up/Down) clamps at the top/bottom row
    /// when `EdgeBehavior::Clamp` is set.
    #[test]
    fn test_next_submenu_selection_vertical_clamp() {
        let grid = dense_3x2_grid();
        assert_eq!(
            next_submenu_selection(
                &grid,
                Some(2),
                Some(1),
                NavigationDirection::Down,
                EdgeBehavior::Clamp,
            ),
            (Some(5), Some(1)),
            "Down preserves the anchor column",
        );
        assert_eq!(
            next_submenu_selection(
                &grid,
                Some(5),
                Some(1),
                NavigationDirection::Down,
                EdgeBehavior::Clamp,
            ),
            (Some(5), Some(1)),
            "Down at the bottom row clamps in place",
        );
        assert_eq!(
            next_submenu_selection(
                &grid,
                Some(1),
                Some(0),
                NavigationDirection::Up,
                EdgeBehavior::Clamp,
            ),
            (Some(1), Some(0)),
            "Up at the top row clamps in place",
        );
    }

    /// Verifies wrap edge behavior wraps horizontally within a row and
    /// vertically within a column.
    #[test]
    fn test_next_submenu_selection_wrap() {
        let grid = dense_3x2_grid();
        assert_eq!(
            next_submenu_selection(
                &grid,
                Some(3),
                Some(2),
                NavigationDirection::Right,
                EdgeBehavior::Wrap,
            ),
            (Some(1), Some(0)),
            "Right at the right edge wraps to the leftmost of the same row",
        );
        assert_eq!(
            next_submenu_selection(
                &grid,
                Some(1),
                Some(0),
                NavigationDirection::Up,
                EdgeBehavior::Wrap,
            ),
            (Some(4), Some(0)),
            "Up at the top row wraps to the bottom of the same column",
        );
    }

    /// Verifies an empty grid returns `(None, None)` regardless of
    /// direction.
    #[test]
    fn test_next_submenu_selection_empty_grid_returns_none() {
        let grid = ClientGrid::from_tiled_pids(&[], 0, 1, 1);

        for direction in [
            NavigationDirection::Up,
            NavigationDirection::Down,
            NavigationDirection::Left,
            NavigationDirection::Right,
        ] {
            assert_eq!(
                next_submenu_selection(&grid, None, None, direction, EdgeBehavior::Clamp,),
                (None, None),
            );
            assert_eq!(
                next_submenu_selection(&grid, Some(1), Some(0), direction, EdgeBehavior::Clamp,),
                (None, None),
            );
        }
    }

    /// Verifies that a Down+Up roundtrip across the partial-last-row
    /// boundary returns to the starting cell from every upper-row
    /// column. With 7 cells laid out as 4 cols x 2 rows (last row has 3
    /// cells stretched to span the full width), Down lands on the
    /// last-row cell whose x-extent contains the anchor column's
    /// centerline, and Up returns to the original column because the
    /// anchor is preserved across the vertical step.
    #[test]
    fn test_grid_down_up_roundtrip_partial_last_row() {
        let grid = ClientGrid::from_tiled_pids(
            &[
                (10, 0),
                (20, 1),
                (30, 2),
                (40, 3),
                (50, 4),
                (60, 5),
                (70, 6),
            ],
            7,
            4,
            2,
        );

        for start in [10u32, 20, 30, 40] {
            let start_col = grid.cell(start).unwrap().col;
            let (mid_pid, mid_anchor) = next_submenu_selection(
                &grid,
                Some(start),
                Some(start_col),
                NavigationDirection::Down,
                EdgeBehavior::Clamp,
            );
            let (round_pid, _) = next_submenu_selection(
                &grid,
                mid_pid,
                mid_anchor,
                NavigationDirection::Up,
                EdgeBehavior::Clamp,
            );
            assert_eq!(
                round_pid,
                Some(start),
                "Down+Up from upper-row col {start_col} must return to PID {start}",
            );
        }
    }

    /// Verifies that re-anchoring kicks in when `current_pid` is gone
    /// from the grid (the case the background `retain` produces) and
    /// returns the first surviving cell with a fresh anchor.
    #[test]
    fn test_next_submenu_selection_reanchors_on_missing_pid() {
        let grid = ClientGrid::from_tiled_pids(&[(1, 0), (2, 1), (3, 2)], 3, 3, 1);

        assert_eq!(
            next_submenu_selection(
                &grid,
                Some(999),
                Some(2),
                NavigationDirection::Up,
                EdgeBehavior::Clamp,
            ),
            (Some(1), Some(0)),
            "missing PID must re-anchor on the first surviving cell with anchor reset to its col",
        );
    }

    /// Verifies the grid-dimension formula stays in sync with the
    /// tiler. The tiler is the spec.
    #[test]
    fn test_grid_dimensions_matches_tiler_formula() {
        // aspect ratio 1.78 ~ 16:9, adjustment 0.0 (square-ish).
        assert_eq!(grid_dimensions(7, 1.78, 0.0), (4, 2));
        assert_eq!(grid_dimensions(9, 1.78, 0.0), (5, 2));
        assert_eq!(grid_dimensions(1, 1.78, 0.0), (1, 1));
    }

    /// Regression: when a client window closes without a retile, the
    /// surviving windows do not move on screen. The navigation grid
    /// must therefore preserve their original tile indices and the
    /// layout's original `n`, so that horizontal navigation across the
    /// gap matches what the user sees.
    #[test]
    fn test_grid_preserves_layout_after_retain() {
        // 6 cells laid out 3x2; client 1 closes without retile.
        let grid = ClientGrid::from_tiled_pids(&[(2, 1), (3, 2), (4, 3), (5, 4), (6, 5)], 6, 3, 2);

        // Client 2 keeps its original cell (row 0, col 1).
        let cell_2 = grid.cell(2).unwrap();
        assert_eq!((cell_2.row, cell_2.col), (0, 1));

        // Right from client 2 lands on client 3 at (row 0, col 2), not
        // on client 4 across the row boundary.
        assert_eq!(
            next_submenu_selection(
                &grid,
                Some(2),
                Some(1),
                NavigationDirection::Right,
                EdgeBehavior::Clamp,
            ),
            (Some(3), Some(2)),
        );
        // And Right from client 3 clamps in place (row 0 still has no
        // surviving cell at col 3).
        assert_eq!(
            next_submenu_selection(
                &grid,
                Some(3),
                Some(2),
                NavigationDirection::Right,
                EdgeBehavior::Clamp,
            ),
            (Some(3), Some(2)),
        );
    }

    /// Vertical step into a row whose anchor-column cell has been
    /// closed lands on the nearest surviving cell in that row.
    #[test]
    fn test_grid_vertical_snap_into_gap() {
        // 6 cells laid out 3x2; close client 5 (row 1, col 1).
        let grid = ClientGrid::from_tiled_pids(&[(1, 0), (2, 1), (3, 2), (4, 3), (6, 5)], 6, 3, 2);

        // Down from client 2 (anchor col=1) would target row 1 col 1,
        // but that cell is gone. Snap to nearest surviving cell in row
        // 1: client 4 (col 0, distance 1) wins the tiebreak against
        // client 6 (col 2, distance 1) by having the smaller col.
        assert_eq!(
            next_submenu_selection(
                &grid,
                Some(2),
                Some(1),
                NavigationDirection::Down,
                EdgeBehavior::Clamp,
            ),
            (Some(4), Some(1)),
            "Down into a gap snaps to nearest by col, tiebreak left",
        );
    }

    /// `Clients::reset_tile_layout` makes the grid match a freshly
    /// retiled screen: dense `tile_index` values and an updated
    /// `layout_n`.
    #[test]
    fn test_clients_reset_tile_layout_dense_renumber() {
        let mut clients = Clients::new();
        for pid in [10u32, 20, 30, 40, 50, 60] {
            clients.push(make_client_with_state(pid, ClientState::Active));
        }
        // Drop two clients without retile.
        clients.retain(|c| return c.process_id != 20 && c.process_id != 50);
        // The surviving clients keep their original tile_index.
        let pre_retile: Vec<usize> = clients.iter().map(|c| return c.tile_index).collect();
        assert_eq!(pre_retile, vec![0, 2, 3, 5]);
        assert_eq!(clients.layout_n, 6);

        // Simulate a retile.
        let surviving_pids: Vec<u32> = clients.iter().map(|c| return c.process_id).collect();
        clients.reset_tile_layout(&surviving_pids);

        let post_retile: Vec<usize> = clients.iter().map(|c| return c.tile_index).collect();
        assert_eq!(post_retile, vec![0, 1, 2, 3]);
        assert_eq!(clients.layout_n, 4);
    }

    /// Verifies that the dispatch arm for `Navigate(Down)` calls
    /// through `next_submenu_pid` and triggers a re-render (which
    /// performs the console calls stubbed on the mock).
    #[test]
    fn test_submenu_navigate_down_advances_selection_via_dispatch() {
        let mut clients = Clients::new();
        clients.push(make_client_with_state(1, ClientState::Active));
        clients.push(make_client_with_state(2, ClientState::Active));
        clients.push(make_client_with_state(3, ClientState::Active));
        let clients = Mutex::new(clients);

        let config = DaemonConfig::default();
        let clusters: Vec<Cluster> = Vec::new();
        let mut daemon = Daemon::for_test(
            &config,
            &clusters,
            ControlModeState::EnableDisableSubmenu {
                highlighted_pid: Some(1),
                anchor_col: Some(0),
            },
        );

        daemon.handle_enable_disable_submenu_key(
            &mock_with_clear_screen(),
            &clients,
            &test_workspace_area(),
            submenu_key_event(VK_DOWN),
        );

        assert_eq!(
            daemon.control_mode_state,
            ControlModeState::EnableDisableSubmenu {
                highlighted_pid: Some(2),
                anchor_col: Some(0),
            }
        );
    }

    /// Verifies that `VK_E` targets the *selected* client rather
    /// than always the first one - the regression the navigation
    /// feature is designed to enable.
    #[test]
    fn test_submenu_e_targets_non_zero_selected_index() {
        let mut clients = Clients::new();
        clients.push(make_client_with_state(1, ClientState::Disabled));
        clients.push(make_client_with_state(2, ClientState::Disabled));
        clients.push(make_client_with_state(3, ClientState::Disabled));
        let clients = Mutex::new(clients);

        let config = DaemonConfig::default();
        let clusters: Vec<Cluster> = Vec::new();
        let mut daemon = Daemon::for_test(
            &config,
            &clusters,
            ControlModeState::EnableDisableSubmenu {
                highlighted_pid: Some(2),
                anchor_col: Some(0),
            },
        );

        daemon.handle_enable_disable_submenu_key(
            &mock_no_calls(),
            &clients,
            &test_workspace_area(),
            submenu_key_event(VK_E),
        );

        assert_eq!(
            snapshot_states(&clients.lock().unwrap()),
            vec![
                ClientState::Disabled,
                ClientState::Active,
                ClientState::Disabled,
            ],
            "VK_E with selection at index 1 must enable only client 1",
        );
    }

    /// Collects every client's `highlight_sender` value in insertion order.
    fn snapshot_highlights(clients: &Clients) -> Vec<bool> {
        return clients
            .iter()
            .map(|client| return *client.highlight_sender.borrow())
            .collect();
    }

    /// Verifies that opening the enable/disable submenu pushes
    /// `highlight_sender = true` on the first client and leaves the
    /// others cleared - the visual signal that drives the new
    /// per-client highlight color.
    #[test]
    fn test_open_enable_disable_submenu_highlights_first_client() {
        let mut clients = Clients::new();
        clients.push(make_client_with_state(1, ClientState::Active));
        clients.push(make_client_with_state(2, ClientState::Active));
        clients.push(make_client_with_state(3, ClientState::Active));
        let clients = Mutex::new(clients);

        let config = DaemonConfig::default();
        let clusters: Vec<Cluster> = Vec::new();
        let daemon = Daemon::for_test(&config, &clusters, ControlModeState::Active);

        let clients_guard = clients.lock().unwrap();
        daemon.apply_submenu_highlight(&clients_guard, None, Some(1));

        assert_eq!(
            snapshot_highlights(&clients_guard),
            vec![true, false, false],
            "opening the submenu must highlight only the first client",
        );
    }

    /// Verifies that the `Navigate(Down)` dispatch arm moves the
    /// per-client highlight from the previously selected index to
    /// the new one.
    #[test]
    fn test_submenu_navigate_moves_highlight() {
        let mut clients = Clients::new();
        clients.push(make_client_with_state(1, ClientState::Active));
        clients.push(make_client_with_state(2, ClientState::Active));
        clients.push(make_client_with_state(3, ClientState::Active));
        // Start with client 0 highlighted, matching the state just
        // after `OpenEnableDisableSubmenu`.
        clients.first().unwrap().highlight_sender.send_replace(true);
        let clients = Mutex::new(clients);

        let config = DaemonConfig::default();
        let clusters: Vec<Cluster> = Vec::new();
        let mut daemon = Daemon::for_test(
            &config,
            &clusters,
            ControlModeState::EnableDisableSubmenu {
                highlighted_pid: Some(1),
                anchor_col: Some(0),
            },
        );

        daemon.handle_enable_disable_submenu_key(
            &mock_with_clear_screen(),
            &clients,
            &test_workspace_area(),
            submenu_key_event(VK_DOWN),
        );

        assert_eq!(
            daemon.control_mode_state,
            ControlModeState::EnableDisableSubmenu {
                highlighted_pid: Some(2),
                anchor_col: Some(0),
            }
        );
        assert_eq!(
            snapshot_highlights(&clients.lock().unwrap()),
            vec![false, true, false],
            "Navigate(Down) must clear the old highlight and set the new one",
        );
    }

    /// Verifies that `apply_submenu_highlight(.., None)` -
    /// the path the `Esc` arm in `control_mode_is_active` takes when
    /// leaving the submenu - clears the highlight on every client.
    #[test]
    fn test_submenu_esc_clears_highlight() {
        let mut clients = Clients::new();
        clients.push(make_client_with_state(1, ClientState::Active));
        clients.push(make_client_with_state(2, ClientState::Active));
        clients.push(make_client_with_state(3, ClientState::Active));
        clients.get(1).unwrap().highlight_sender.send_replace(true);
        let clients = Mutex::new(clients);

        let config = DaemonConfig::default();
        let clusters: Vec<Cluster> = Vec::new();
        let daemon = Daemon::for_test(
            &config,
            &clusters,
            ControlModeState::EnableDisableSubmenu {
                highlighted_pid: Some(2),
                anchor_col: Some(0),
            },
        );

        let clients_guard = clients.lock().unwrap();
        daemon.apply_submenu_highlight(&clients_guard, Some(2), None);

        assert_eq!(
            snapshot_highlights(&clients_guard),
            vec![false, false, false],
            "Esc must clear the highlight on the previously-selected client",
        );
    }

    /// Regression test: when an exited client is retained-out
    /// mid-submenu, the same numeric index now points at a
    /// different client. The new occupant of the selected index
    /// must still receive `highlight_sender = true` even though the
    /// index value did not change.
    #[test]
    fn test_apply_submenu_highlight_handles_index_reuse_after_retain() {
        let mut clients = Clients::new();
        // Two clients, the first is highlighted (state right after
        // `OpenEnableDisableSubmenu`).
        clients.push(make_client_with_state(1, ClientState::Active));
        clients.push(make_client_with_state(2, ClientState::Active));
        clients.first().unwrap().highlight_sender.send_replace(true);

        // The background monitor would call `retain` to remove the
        // exited client; do the same here.
        clients.retain(|client| return client.process_id != 1);
        assert_eq!(clients.len(), 1);
        // The surviving client (PID 2) was never highlighted: its
        // `highlight_sender` is still `false`.
        assert!(!*clients.first().unwrap().highlight_sender.borrow());

        let config = DaemonConfig::default();
        let clusters: Vec<Cluster> = Vec::new();
        // The daemon thought PID 1 was highlighted before retain;
        // re-anchoring on the surviving PID 2 must turn its highlight on.
        let daemon = Daemon::for_test(
            &config,
            &clusters,
            ControlModeState::EnableDisableSubmenu {
                highlighted_pid: Some(1),
                anchor_col: Some(0),
            },
        );

        daemon.apply_submenu_highlight(&clients, Some(1), Some(2));

        assert_eq!(
            snapshot_highlights(&clients),
            vec![true],
            "after retain reuses an index the surviving client must be highlighted",
        );
    }

    /// Regression test: when a client BEFORE the selected one exits
    /// mid-submenu, surviving clients slide to lower indices but
    /// the previously-highlighted client must still be cleared on
    /// the next navigation. PID-based tracking handles this; the
    /// previous index-based clear would have left the highlight
    /// stale on the shifted client.
    #[test]
    fn test_apply_submenu_highlight_clears_shifted_client_after_retain() {
        let mut clients = Clients::new();
        // Three clients, the second is highlighted.
        clients.push(make_client_with_state(1, ClientState::Active));
        clients.push(make_client_with_state(2, ClientState::Active));
        clients.push(make_client_with_state(3, ClientState::Active));
        clients.get(1).unwrap().highlight_sender.send_replace(true);

        // Drop the FIRST client - PID 2 (still highlighted) shifts
        // to index 0, PID 3 shifts to index 1.
        clients.retain(|client| return client.process_id != 1);
        assert_eq!(clients.len(), 2);
        assert_eq!(
            snapshot_highlights(&clients),
            vec![true, false],
            "precondition: PID 2 stayed highlighted after retain shifted it to index 0",
        );

        let config = DaemonConfig::default();
        let clusters: Vec<Cluster> = Vec::new();
        let daemon = Daemon::for_test(
            &config,
            &clusters,
            ControlModeState::EnableDisableSubmenu {
                highlighted_pid: Some(2),
                anchor_col: Some(0),
            },
        );

        // User navigates `Down`: the daemon now wants PID 3
        // highlighted. The clear half must find PID 2 by id and turn
        // it off, even though it is no longer at the index the daemon
        // last saw it at.
        daemon.apply_submenu_highlight(&clients, Some(2), Some(3));

        assert_eq!(
            snapshot_highlights(&clients),
            vec![false, true],
            "the previously highlighted client must be cleared even after a retain shift",
        );
    }

    /// Regression test: when the selected client (and others past it)
    /// are retained-out mid-submenu, the `highlighted_pid` carried on
    /// the variant no longer maps to any surviving client. An Up/Left
    /// navigation must still re-anchor on a surviving client and
    /// propagate the highlight to it instead of silently leaving the
    /// submenu untargeted.
    #[test]
    fn test_submenu_navigate_clamps_stale_index_after_retain() {
        let mut clients = Clients::new();
        clients.push(make_client_with_state(1, ClientState::Active));
        clients.push(make_client_with_state(2, ClientState::Active));
        clients.push(make_client_with_state(3, ClientState::Active));
        // Submenu opened with the last client highlighted.
        clients.get(2).unwrap().highlight_sender.send_replace(true);

        // The background monitor retains-out the last two clients
        // while the submenu is open, leaving the highlighted PID stale.
        clients.retain(|client| return client.process_id == 1);
        assert_eq!(clients.len(), 1);
        let clients = Mutex::new(clients);

        let config = DaemonConfig::default();
        let clusters: Vec<Cluster> = Vec::new();
        let mut daemon = Daemon::for_test(
            &config,
            &clusters,
            ControlModeState::EnableDisableSubmenu {
                highlighted_pid: Some(3),
                anchor_col: Some(0),
            },
        );

        daemon.handle_enable_disable_submenu_key(
            &mock_with_clear_screen(),
            &clients,
            &test_workspace_area(),
            submenu_key_event(VK_UP),
        );

        assert_eq!(
            daemon.control_mode_state,
            ControlModeState::EnableDisableSubmenu {
                highlighted_pid: Some(1),
                anchor_col: Some(0),
            },
            "Up navigation from a stale PID must re-anchor on the surviving client",
        );
        assert_eq!(
            snapshot_highlights(&clients.lock().unwrap()),
            vec![true],
            "the surviving client must receive the highlight after re-anchor",
        );
    }
}
