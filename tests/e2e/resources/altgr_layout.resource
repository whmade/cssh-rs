*** Settings ***
Documentation       Keywords shared by the AltGr suites, which run the daemon's console on
...                 one keyboard layout for a whole suite.
...
...                 The hosts read their input as raw virtual terminal input, so ESC and
...                 other control bytes reach the markers as the client sent them; the
...                 default cooked read would let line editing consume them.

Resource            cssh_rs_cluster.resource
Library             cssh_rs_automation.keyboard_layout.KeyboardLayout


*** Keywords ***
Start Cluster On Layout
    [Documentation]    Start the cluster with raw-input markers, wait for every window and
    ...                ssh session, then switch the daemon's console to layout ${klid}. The
    ...                daemon is focused first: Windows carries a layout switch made in the
    ...                foreground window into every window focused after it, but overrides
    ...                one made in a background window as soon as that window gets focus.
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
    [Documentation]    Tear the cluster down, switch back to the layout from before the suite
    ...                in a spare console window, so a crashed daemon cannot block it, then
    ...                stop the whole run if the layout did not come back. Restoring does
    ...                nothing when the setup never switched.
    Tear Down Cssh Cluster
    ${status}    ${error}=    Run Keyword And Ignore Error    Restore Keyboard Layout
    IF    $status == 'FAIL'
        # Since Windows 8 the layout follows the user into every window focused later,
        # so later suites would run on this suite's layout and their failures could be
        # the layout's rather than cssh-rs's, making them pointless to run:
        # https://learn.microsoft.com/en-us/previous-versions/hh994466(v=vs.85)
        Fatal Error    Could not restore the keyboard layout from before this suite: ${error}
    END

Assert Every Host Received Bytes
    [Documentation]    Assert every host's marker holds ${bytes} exactly, control bytes
    ...                included. Put unique text on both sides of the bytes under test: the
    ...                line check cannot see them, since a lost ESC leaves a whole line and
    ...                0x1c and 0x1d split a line.
    [Arguments]    ${bytes}
    FOR    ${alias}    IN    @{ALIASES}
        ${content}=    Read Marker    ${alias}
        Should Contain    ${content}    ${bytes}
        ...    msg=Marker for ${alias} lacks ${{ascii($bytes)}}: ${{ascii($content)}}
        ...    values=False
    END

Keys Typed In Daemon Reach Every Host As
    [Documentation]    Type unique text into the daemon, press ${keys} with real scan codes,
    ...                type more unique text and Enter, then assert every host received
    ...                ${bytes} between the two texts.
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
