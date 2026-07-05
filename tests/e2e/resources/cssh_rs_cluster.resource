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
    ${binary_dir}=    Evaluate    os.path.dirname(r"${CSSH_RS_BINARY}")    modules=os
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

Fan Out Line Reaches All Hosts
    [Documentation]    Focus the daemon, type ${text}, and assert it reached every host's
    ...                marker. Retried by the caller until the ssh sessions are connected;
    ...                duplicate appends from earlier attempts are harmless because the
    ...                assertion is a substring check.
    [Arguments]    ${text}
    Focus Window    ${DAEMON_TITLE}
    Type Line    ${text}
    FOR    ${alias}    IN    @{ALIASES}
        ${content}=    Read Marker    ${alias}
        Should Contain    ${content}    ${text}
    END

Client Line Reaches Only
    [Documentation]    Focus the ${target} client window, type ${text}, and assert it
    ...                reached that host's marker. The negative half (other hosts stay
    ...                clean) is asserted by the test after this confirms delivery.
    [Arguments]    ${target}    ${text}
    Focus Window    @${target}    substring
    Type Line    ${text}
    ${content}=    Read Marker    ${target}
    Should Contain    ${content}    ${text}
