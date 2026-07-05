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

    ${all}=    Unique Message    ENABLEDALL
    Focus Daemon And Broadcast    ${all}
    Wait Until Keyword Succeeds    10x    0.5s    Assert Every Host Received    ${all}

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
    # By the time the re-enabled broadcast has landed everywhere, any leak of
    # the silenced one would have arrived too.
    Wait Until Keyword Succeeds    10x    0.5s    Assert Every Host Received    ${reenabled}
    Assert No Host Received    ${silenced}


*** Keywords ***
Navigate To Second Client
    [Documentation]    Step from the default top-left selection to the second client. With
    ...                two clients one of right/down moves and the other clamps, so this
    ...                lands on the second client regardless of the grid orientation.
    Send Control Mode Key    right
    Send Control Mode Key    down

Disable Second Client
    [Documentation]    Open the submenu, navigate to the second client and disable it with
    ...                [d], then leave control mode.
    Enter Control Mode
    Send Control Mode Key    e
    Navigate To Second Client
    Send Control Mode Key    d
    Exit Control Mode

Enable Second Client
    [Documentation]    Re-open the submenu, navigate to the second client and enable it with
    ...                [e], then leave control mode.
    Enter Control Mode
    Send Control Mode Key    e
    Navigate To Second Client
    Send Control Mode Key    e
    Exit Control Mode

Disable All Clients
    [Documentation]    Force every client enabled with [n], then toggle all with [t] so
    ...                they all end disabled regardless of prior state.
    Enter Control Mode
    Send Control Mode Key    n
    Enter Control Mode
    Send Control Mode Key    t

Enable All Clients
    [Documentation]    Force every client back to enabled with [n].
    Enter Control Mode
    Send Control Mode Key    n
