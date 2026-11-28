*** Settings ***
Documentation       Keywords shared by the AltGr suites.

Resource            raw_input_cluster.resource
Library             cssh_rs_automation.keyboard_layout.KeyboardLayout


*** Keywords ***
Start Cluster On Layout
    [Documentation]    Start the raw-input cluster, then switch the focused daemon to layout
    ...                ${klid}. Switched in the foreground, the layout carries into every
    ...                window focused later, the clients' included.
    [Arguments]    ${klid}
    Start Raw Input Cluster
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
