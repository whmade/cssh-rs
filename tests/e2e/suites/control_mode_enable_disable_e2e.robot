*** Settings ***
Documentation       Windows E2E case: enabling, disabling and toggling clients in control
...                 mode. Exercises the submenu [d]/[e]/[t] on a navigated-to client and the
...                 top-level [n] enable-all and [t] toggle-all, asserting broadcast targeting
...                 follows each client's enabled state.

Resource            ../resources/cssh_rs_cluster.resource

Suite Setup         Run Keywords    Start Suite Recording    AND    Start Cssh Cluster
Suite Teardown      Run Keywords    Tear Down Cssh Cluster    AND    Stop Suite Recording


*** Variables ***
${CSSH_RS_BINARY}       %{CSSH_RS_BINARY}
@{ALIASES}              alpha    bravo
${SECOND_HOST}          bravo
${CLUSTER_NAME}         e2e


*** Test Cases ***
Enable Disable And Toggle Controls Broadcast Targeting
    [Documentation]    Baseline reaches both. Submenu [d] silences the second client, [n]
    ...                restores all, submenu [t] silences that client again, [e] restores it,
    ...                and top-level [t] silences all then restores all.
    Assert Daemon Window Appears
    Assert Client Window Appears For Each Host
    Assert All Ssh Connections Established

    ${baseline}=    Unique Message    BASELINE
    Focus Daemon And Broadcast    ${baseline}
    Wait Until Keyword Succeeds    10x    0.5s    Assert Every Host Received    ${baseline}

    Disable Second Client
    ${disabled}=    Unique Message    DISABLED
    Focus Daemon And Broadcast    ${disabled}
    Wait Until Keyword Succeeds    10x    0.5s    Assert Only Host Missing    ${SECOND_HOST}    ${disabled}

    Enable All Clients
    ${enabled}=    Unique Message    ENABLED
    Focus Daemon And Broadcast    ${enabled}
    Wait Until Keyword Succeeds    10x    0.5s    Assert Every Host Received    ${enabled}

    Toggle Second Client
    ${toggled_off}=    Unique Message    TOGGLEDOFF
    Focus Daemon And Broadcast    ${toggled_off}
    Wait Until Keyword Succeeds    10x    0.5s    Assert Only Host Missing    ${SECOND_HOST}    ${toggled_off}

    Enable Second Client
    ${restored}=    Unique Message    RESTORED
    Focus Daemon And Broadcast    ${restored}
    Wait Until Keyword Succeeds    10x    0.5s    Assert Every Host Received    ${restored}

    Toggle All Clients
    ${silenced}=    Unique Message    SILENCED
    Focus Daemon And Broadcast    ${silenced}
    Toggle All Clients
    ${reenabled}=    Unique Message    REENABLED
    Focus Daemon And Broadcast    ${reenabled}
    # Any leak of the silenced broadcast would have arrived before the re-enabled one lands.
    Wait Until Keyword Succeeds    10x    0.5s    Assert Every Host Received    ${reenabled}
    Assert No Host Received    ${silenced}


*** Keywords ***
Navigate To Second Client
    [Documentation]    Move the submenu selection from its default top-left to the second
    ...                client. One of right/down moves and the other clamps, so this works
    ...                for either grid orientation.
    Send Control Mode Key    right
    Send Control Mode Key    down

Disable Second Client
    [Documentation]    Disable the second client through the submenu [d].
    Enter Control Mode
    Send Control Mode Key    e
    Navigate To Second Client
    Send Control Mode Key    d
    Exit Control Mode

Enable Second Client
    [Documentation]    Enable the second client through the submenu [e].
    Enter Control Mode
    Send Control Mode Key    e
    Navigate To Second Client
    Send Control Mode Key    e
    Exit Control Mode

Toggle Second Client
    [Documentation]    Toggle the second client's enabled state through the submenu [t].
    Enter Control Mode
    Send Control Mode Key    e
    Navigate To Second Client
    Send Control Mode Key    t
    Exit Control Mode

Enable All Clients
    [Documentation]    Enable every client with the top-level [n].
    Enter Control Mode
    Send Control Mode Key    n

Toggle All Clients
    [Documentation]    Toggle every client's enabled state with the top-level [t].
    Enter Control Mode
    Send Control Mode Key    t
