*** Settings ***
Documentation       Keywords shared by the AltGr suites. Their hosts read raw virtual
...                 terminal input, so ESC and control bytes reach the markers instead of
...                 being consumed by line editing.

Resource            cssh_rs_cluster.resource
Library             cssh_rs_automation.keyboard_layout.KeyboardLayout


*** Keywords ***
Start Cluster On Layout
    [Documentation]    Start the cluster with raw-input markers, wait until every window and
    ...                ssh session is up, then switch the focused daemon to layout ${klid}.
    ...                Switched in the foreground, the layout carries into every window
    ...                focused later, the clients' included.
    [Arguments]    ${klid}
    ${raw_markers}=    Evaluate    cssh_rs_automation.sshd_fixture.MarkerMode(raw_input=True)
    ...    modules=cssh_rs_automation.sshd_fixture
    Start Cssh Cluster    sshd_mode=${raw_markers}
    Assert Daemon Window Appears
    Assert Client Window Appears For Each Host
    Assert All Ssh Connections Established
    Focus Window    ${DAEMON_TITLE}
    Switch Keyboard Layout    ${DAEMON_TITLE}    ${klid}

Tear Down Cluster And Restore Layout
    [Documentation]    Tear the cluster down and restore the layout from before the suite,
    ...                stopping the whole run if that fails.
    Tear Down Cssh Cluster
    ${status}    ${error}=    Run Keyword And Ignore Error    Restore Keyboard Layout
    IF    $status == 'FAIL'
        # The layout follows the user, so every later suite would run on it and prove nothing:
        # https://learn.microsoft.com/en-us/previous-versions/hh994466(v=vs.85)
        Fatal Error    Could not restore the keyboard layout from before this suite: ${error}
    END

Assert Every Host Received Bytes
    [Documentation]    Assert every host's marker contains ${bytes}, a substring check
    ...                because Split To Lines breaks lines at 0x1c and 0x1d.
    [Arguments]    ${bytes}
    FOR    ${alias}    IN    @{ALIASES}
        ${content}=    Read Marker    ${alias}
        Should Contain    ${content}    ${bytes}
        ...    msg=Marker for ${alias} lacks ${{ascii($bytes)}}: ${{ascii($content)}}
        ...    values=False
    END

Keys Typed In Daemon Reach Every Host As
    [Documentation]    Press ${keys} by scan code in the daemon between two unique texts,
    ...                then assert every host received ${bytes} exactly between them.
    [Arguments]    ${bytes}    @{keys}
    ${prefix}=    Unique Message    BEFORE
    ${suffix}=    Unique Message    AFTER
    Focus Window    ${DAEMON_TITLE}
    Type Text    ${prefix}
    Press Keys With Scan Codes    @{keys}
    Type Text    ${suffix}
    Press Key    enter
    Wait Until Keyword Succeeds    10x    0.5s
    ...    Assert Every Host Received Bytes    ${prefix}${bytes}${suffix}
