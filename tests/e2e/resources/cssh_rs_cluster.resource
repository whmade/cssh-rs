*** Settings ***
Documentation       Reusable keywords for the cssh-rs Windows E2E cluster suite.
...
...                 Composes the shipped harness libraries (sshd fixture, config
...                 generation, window focus, keystrokes) into the setup, teardown
...                 and per-scenario actions the suite drives. All readiness is
...                 polled by the suite with Wait Until Keyword Succeeds; these
...                 keywords hold no sleeps.

Library             cssh_rs_automation.sshd_fixture.SshdFixture
Library             cssh_rs_automation.config_gen.ConfigGen
Library             cssh_rs_automation.window_focus.WindowFocus
Library             cssh_rs_automation.keystrokes.Keystrokes
Library             cssh_rs_automation.clipboard.Clipboard
Library             Process
Library             OperatingSystem
Library             String


*** Variables ***
${DAEMON_TITLE}     cssh-rs daemon


*** Keywords ***
Start Cssh Cluster
    [Documentation]    Start sshd, generate the config into the binary's own directory
    ...                (cssh-rs loads cssh-rs-config.toml from there), and launch the
    ...                cluster. Stores suite variables for the tests and Tear Down Cssh
    ...                Cluster. ${cluster_aliases} are the hosts the cluster launches;
    ...                ${sshd_aliases} are the aliases sshd serves and defaults to
    ...                ${cluster_aliases}. Pass a superset as ${sshd_aliases} to leave
    ...                spare hosts a test can add at runtime. ${grant_pty} opts into a
    ...                PTY-granting session so a relayed Ctrl+C reaches the remote.
    [Arguments]    ${cluster_aliases}=${ALIASES}    ${sshd_aliases}=${NONE}    ${grant_pty}=${FALSE}
    IF    $sshd_aliases is None
        ${sshd_aliases}=    Set Variable    ${cluster_aliases}
    END
    ${sshd}=    Start Sshd    ${sshd_aliases}    grant_pty=${grant_pty}
    Set Suite Variable    ${SSHD}    ${sshd}
    ${binary_dir}=    Evaluate    os.path.dirname(os.path.abspath(r"${CSSH_RS_BINARY}"))    modules=os
    ${config_path}=    Generate Config    ${CSSH_RS_BINARY}    ${binary_dir}
    ...    ${sshd}[ssh_config]    ${cluster_aliases}    cluster_name=${CLUSTER_NAME}
    Set Suite Variable    ${CONFIG_PATH}    ${config_path}
    Start Process    ${CSSH_RS_BINARY}    ${CLUSTER_NAME}

Tear Down Cssh Cluster
    [Documentation]    Stop cssh-rs and sshd and verify the per-run temp tree is gone.
    ...                Each step is best-effort so one failure - or an early Suite Setup
    ...                failure that left the suite variables unset - cannot mask the rest.
    Run Keyword And Ignore Error    Stop Cssh Process
    ${config_path}=    Get Variable Value    ${CONFIG_PATH}    ${NONE}
    IF    $config_path is not None
        Run Keyword And Ignore Error    Remove File    ${config_path}
    END
    Run Keyword And Ignore Error    Stop Sshd
    ${sshd}=    Get Variable Value    ${SSHD}    ${NONE}
    ${keep}=    Get Environment Variable    CSSH_E2E_KEEP_TEMP    ${EMPTY}
    IF    $sshd is not None and '${keep}' != '1'
        Directory Should Not Exist    ${sshd}[tempdir]
    END

Restart Cssh Cluster
    [Documentation]    Tear the current cluster down before starting a fresh one: the
    ...                single-instance sshd fixture refuses to start while one runs.
    Tear Down Cssh Cluster
    Start Cssh Cluster

Restart Cssh Cluster With Pty
    [Documentation]    Restart into a PTY-granting cluster so a relayed Ctrl+C forwards to the
    ...                remote as a real interrupt instead of being swallowed by the local ssh.
    Tear Down Cssh Cluster
    Start Cssh Cluster    grant_pty=${TRUE}

Stop Cssh Process
    [Documentation]    Kill the daemon, every client and their ssh children by image name.
    ...                The launcher spawns a detached daemon console, so killing the
    ...                launched process alone would leave the tree running.
    ${image}=    Evaluate    os.path.basename(r"${CSSH_RS_BINARY}")    modules=os
    Run Process    taskkill    /F    /T    /IM    ${image}

Unique Message
    [Documentation]    Return ${prefix} with a random suffix so scenarios sharing the suite
    ...                fixture cannot cross-contaminate each other's marker assertions.
    [Arguments]    ${prefix}
    ${suffix}=    Generate Random String    16    [LOWER][NUMBERS]
    RETURN    ${prefix}${suffix}

Assert Daemon Window Appears
    [Documentation]    Wait until the daemon window can be focused; a unique title match
    ...                proves exactly one such window exists.
    Wait Until Keyword Succeeds    15x    1s    Focus Window    ${DAEMON_TITLE}

Assert Client Window Appears For Each Host
    [Documentation]    Wait until one client window per host can be focused.
    FOR    ${alias}    IN    @{ALIASES}
        Wait Until Keyword Succeeds    15x    1s    Focus Window    @${alias}    substring
    END

Assert All Ssh Connections Established
    [Documentation]    Wait until every host's ssh session has authenticated so typed
    ...                input is not dropped into a session that has not connected yet.
    ${count}=    Get Length    ${ALIASES}
    Wait Until Keyword Succeeds    30x    1s    All Connections Reported    ${count}

All Connections Reported
    [Documentation]    Assert at least ${expected} host markers exist, one per connected session.
    [Arguments]    ${expected}
    ${connected}=    Count Connected Markers
    Should Be True    ${connected} >= ${expected}
    ...    Only ${connected} of ${expected} ssh sessions connected

Focus Daemon And Broadcast
    [Documentation]    Focus the daemon window and type ${message} once. With the daemon
    ...                focused, cssh-rs broadcasts the input to every client's ssh session.
    [Arguments]    ${message}
    Focus Window    ${DAEMON_TITLE}
    Type Line    ${message}

Focus Client And Type
    [Documentation]    Focus the ${target} client window and type ${message} once. A
    ...                focused client console routes input to that client's ssh session only.
    [Arguments]    ${target}    ${message}
    Focus Window    @${target}    substring
    Type Line    ${message}

Assert Every Host Received
    [Documentation]    Assert ${message} arrived as a complete line (exact, not substring)
    ...                in every host's marker.
    [Arguments]    ${message}
    Assert Hosts Received    ${ALIASES}    ${message}

Assert Hosts Received
    [Documentation]    Assert ${message} arrived as a complete line (exact, not substring)
    ...                in the marker of every alias in ${aliases}.
    [Arguments]    ${aliases}    ${message}
    FOR    ${alias}    IN    @{aliases}
        Assert Marker Has Line    ${alias}    ${message}
    END

Assert Only Host Received
    [Documentation]    Assert ${message} reached ${target}'s marker as a complete line and
    ...                no other host's marker.
    [Arguments]    ${target}    ${message}
    Assert Marker Has Line    ${target}    ${message}
    FOR    ${alias}    IN    @{ALIASES}
        IF    '${alias}' != '${target}'
            Assert Marker Missing Line    ${alias}    ${message}
        END
    END

Assert Marker Has Line
    [Arguments]    ${alias}    ${message}
    ${lines}=    Marker Lines    ${alias}
    Should Contain    ${lines}    ${message}    Marker for ${alias} is missing ${message}

Assert Marker Missing Line
    [Arguments]    ${alias}    ${message}
    ${lines}=    Marker Lines    ${alias}
    Should Not Contain    ${lines}    ${message}    Marker for ${alias} unexpectedly has ${message}

Marker Lines
    [Arguments]    ${alias}
    ${content}=    Read Marker    ${alias}
    ${lines}=    Split To Lines    ${content}
    RETURN    ${lines}

Enter Control Mode
    [Documentation]    Focus the daemon and press Ctrl+A so subsequent keys drive the
    ...                control-mode menu instead of broadcasting to clients.
    Focus Window    ${DAEMON_TITLE}
    Send Hotkey    ctrl    a

Send Control Mode Key
    [Documentation]    Press a single control-mode menu key.
    [Arguments]    ${key}
    Press Key    ${key}

Exit Control Mode
    [Documentation]    Press Esc to leave control mode and any open submenu.
    Press Key    esc

Assert No Host Received
    [Documentation]    Assert ${message} reached no host's marker as a complete line.
    [Arguments]    ${message}
    FOR    ${alias}    IN    @{ALIASES}
        Assert Marker Missing Line    ${alias}    ${message}
    END

Assert Only Host Missing
    [Documentation]    Assert ${message} reached every host except ${target}.
    [Arguments]    ${target}    ${message}
    FOR    ${alias}    IN    @{ALIASES}
        IF    '${alias}' == '${target}'
            Assert Marker Missing Line    ${alias}    ${message}
        ELSE
            Assert Marker Has Line    ${alias}    ${message}
        END
    END

Close Daemon Window
    [Documentation]    Terminate only the daemon by its window title, leaving the clients
    ...                to notice the closed pipe and exit on their own.
    Run Process    taskkill    /F    /FI    WINDOWTITLE eq ${DAEMON_TITLE}

Assert Window Gone
    [Documentation]    Assert no window matching ${title} exists. Uses a short focus probe:
    ...                a still-present window would be focused and fail the assertion.
    [Arguments]    ${title}    ${match_mode}=exact
    ${present}=    Run Keyword And Return Status    Focus Window    ${title}    ${match_mode}    2
    Should Not Be True    ${present}    Window '${title}' is still present

Assert Daemon Window Gone
    Assert Window Gone    ${DAEMON_TITLE}

Assert Client Window Gone
    [Arguments]    ${alias}
    Assert Window Gone    @${alias}    substring

Assert All Client Windows Raised Above
    [Documentation]    Assert every client window sits above ${other_title} in the z-order,
    ...                proving the daemon's focus sync raised them all rather than leaving
    ...                any hidden beneath it.
    [Arguments]    ${other_title}    ${other_match_mode}=substring
    ${other_index}=    Window Z Order Index    ${other_title}    ${other_match_mode}
    FOR    ${alias}    IN    @{ALIASES}
        ${client_index}=    Window Z Order Index    @${alias}    substring
        Should Be True    ${client_index} < ${other_index}
        ...    Client window for ${alias} is hidden under ${other_title}
    END
