*** Settings ***
Documentation       Reusable keywords for the cssh-rs Windows E2E cluster suite.
...
...                 Composes the shipped harness libraries (sshd fixture, config
...                 generation, window focus, keystrokes) into the setup, teardown
...                 and per-scenario actions the suite drives. All readiness is
...                 polled by the suite with Wait Until Keyword Succeeds; these
...                 keywords hold no sleeps.

Library             cssh_rs_e2e.sshd_fixture.SshdFixture
Library             cssh_rs_e2e.config_gen.ConfigGen
Library             cssh_rs_e2e.window_focus.WindowFocus
Library             cssh_rs_e2e.keystrokes.Keystrokes
Library             Process
Library             OperatingSystem
Library             String


*** Variables ***
${DAEMON_TITLE}     cssh-rs daemon


*** Keywords ***
Start Cssh Cluster
    [Documentation]    Start sshd for the suite aliases, generate the cssh-rs config into
    ...                the binary's own directory, and launch the cluster. cssh-rs loads
    ...                cssh-rs-config.toml from its exe directory, so the config must be
    ...                written there. Stores suite variables consumed by the tests and by
    ...                Tear Down Cssh Cluster.
    ${sshd}=    Start Sshd    ${ALIASES}
    Set Suite Variable    ${SSHD}    ${sshd}
    ${binary_dir}=    Evaluate    os.path.dirname(os.path.abspath(r"${CSSH_RS_BINARY}"))    modules=os
    ${config_path}=    Generate Config    ${CSSH_RS_BINARY}    ${binary_dir}
    ...    ${sshd}[ssh_config]    ${ALIASES}    cluster_name=${CLUSTER_NAME}
    Set Suite Variable    ${CONFIG_PATH}    ${config_path}
    Start Process    ${CSSH_RS_BINARY}    ${CLUSTER_NAME}    alias=cssh

Tear Down Cssh Cluster
    [Documentation]    Stop cssh-rs and sshd and verify the per-run temp tree is gone.
    ...                The launcher spawns a detached daemon console, so terminating the
    ...                launched process is not enough; taskkill /T /F by image name kills
    ...                the daemon, every client and their ssh children. Each cleanup step
    ...                is best-effort so a single failure does not mask the others or an
    ...                early Suite Setup failure where the suite variables are unset.
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

Stop Cssh Process
    [Documentation]    Kill the daemon, client and ssh process trees by image name.
    ${image}=    Evaluate    os.path.basename(r"${CSSH_RS_BINARY}")    modules=os
    Run Process    taskkill    /F    /T    /IM    ${image}

Unique Message
    [Documentation]    Return ${prefix} with a random suffix so each scenario's typed line
    ...                is unique and cases sharing the suite fixture cannot cross-contaminate.
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
    [Documentation]    Assert sshd reports at least ${expected} accepted connections.
    [Arguments]    ${expected}
    ${connected}=    Count Established Connections
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
    [Documentation]    Assert ${message} arrived as a complete line in every host's marker.
    ...                Matching whole lines (not substrings) keeps the assertion exact.
    [Arguments]    ${message}
    FOR    ${alias}    IN    @{ALIASES}
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
    [Documentation]    Assert ${message} is one of the complete lines in ${alias}'s marker.
    [Arguments]    ${alias}    ${message}
    ${lines}=    Marker Lines    ${alias}
    Should Contain    ${lines}    ${message}    Marker for ${alias} is missing ${message}

Assert Marker Missing Line
    [Documentation]    Assert ${message} is not any complete line in ${alias}'s marker.
    [Arguments]    ${alias}    ${message}
    ${lines}=    Marker Lines    ${alias}
    Should Not Contain    ${lines}    ${message}    Marker for ${alias} unexpectedly has ${message}

Marker Lines
    [Documentation]    Return ${alias}'s marker content split into complete lines.
    [Arguments]    ${alias}
    ${content}=    Read Marker    ${alias}
    ${lines}=    Split To Lines    ${content}
    RETURN    ${lines}
