*** Settings ***
Documentation       Windows E2E case: enabling and disabling clients in control mode.
...                 Exercises the per-client submenu [d]/[e] on a navigated-to client,
...                 toggle-all [t] and enable-all [n], asserting broadcast targeting
...                 follows each client's enabled state.

Resource            ../resources/cssh_rs_cluster.resource

Suite Setup         Start Cssh Cluster
Suite Teardown      Tear Down Cssh Cluster


*** Variables ***
${CSSH_RS_BINARY}       %{CSSH_RS_BINARY}
@{ALIASES}              alpha    bravo
${SECOND_HOST}          bravo
${CLUSTER_NAME}         e2e


*** Test Cases ***
Enable Disable Controls Broadcast Targeting
    [Documentation]    Baseline broadcast reaches both; disabling the navigated-to second
    ...                client silences only it; re-enabling restores it; disabling all
    ...                silences all; enabling all restores all.
    Assert Daemon Window Appears
    Assert Client Window Appears For Each Host
    Assert All Ssh Connections Established

    ${baseline}=    Unique Message    BASELINE
    Focus Daemon And Broadcast    ${baseline}
    Wait Until Keyword Succeeds    10x    0.5s    Assert Every Host Received    ${baseline}

    Disable Second Client
    ${one_off}=    Unique Message    ONEOFF
    Focus Daemon And Broadcast    ${one_off}
    Wait Until Keyword Succeeds    10x    0.5s    Assert Only Host Missing    ${SECOND_HOST}    ${one_off}

    Enable Second Client
    ${restored}=    Unique Message    RESTORED
    Focus Daemon And Broadcast    ${restored}
    Wait Until Keyword Succeeds    10x    0.5s    Assert Every Host Received    ${restored}

    Disable All Clients
    ${silenced}=    Unique Message    SILENCED
    Focus Daemon And Broadcast    ${silenced}
    Enable All Clients
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

Disable All Clients
    [Documentation]    Enable all with [n] then toggle all with [t] so every client ends
    ...                disabled; the menu has no dedicated disable-all.
    Enter Control Mode
    Send Control Mode Key    n
    Enter Control Mode
    Send Control Mode Key    t

Enable All Clients
    [Documentation]    Enable every client with [n].
    Enter Control Mode
    Send Control Mode Key    n
