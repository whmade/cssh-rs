*** Settings ***
Documentation       Windows E2E cases: enabling, disabling and toggling clients in control
...                 mode. Exercises the submenu [d]/[e]/[t] on a navigated-to client and the
...                 top-level [n] enable-all and [t] toggle-all, asserting broadcast targeting
...                 follows each client's enabled state. Every case shares one cluster and
...                 resets to all-enabled on teardown so it starts from a known baseline.

Resource            ../resources/cssh_rs_cluster.resource

Suite Setup         Start Cluster And Await Readiness
Suite Teardown      Tear Down Cssh Cluster
Test Teardown       Enable All Clients


*** Variables ***
${CSSH_RS_BINARY}       %{CSSH_RS_BINARY}
@{ALIASES}              alpha    bravo
${SECOND_HOST}          bravo
${CLUSTER_NAME}         e2e


*** Test Cases ***
Baseline Broadcast Reaches Every Client
    [Documentation]    With every client enabled, a daemon broadcast reaches all hosts.
    Broadcast And Assert Every Host Received    BASELINE

Submenu Disable Silences A Single Client
    [Documentation]    Submenu [d] on the second client stops daemon broadcasts reaching it
    ...                while every other client still receives them.
    Disable Second Client
    ${disabled}=    Unique Message    DISABLED
    Focus Daemon And Broadcast    ${disabled}
    Wait Until Keyword Succeeds    10x    0.5s    Assert Only Host Missing    ${SECOND_HOST}    ${disabled}

Enable All Restores A Disabled Client
    [Documentation]    After disabling the second client, the top-level [n] enable-all brings
    ...                every client back into broadcast targeting.
    Disable Second Client
    Enable All Clients
    Broadcast And Assert Every Host Received    ENABLED

Submenu Toggle Silences And Enable Restores A Client
    [Documentation]    Submenu [t] silences the second client, then submenu [e] restores it.
    Toggle Second Client
    ${toggled_off}=    Unique Message    TOGGLEDOFF
    Focus Daemon And Broadcast    ${toggled_off}
    Wait Until Keyword Succeeds    10x    0.5s    Assert Only Host Missing    ${SECOND_HOST}    ${toggled_off}
    Enable Second Client
    Broadcast And Assert Every Host Received    RESTORED

Toggle All Silences Then Restores Every Client
    [Documentation]    Top-level [t] silences every client and a second [t] restores them; the
    ...                broadcast sent while all were silenced reaches nobody.
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
Start Cluster And Await Readiness
    [Documentation]    Start the cluster and wait for the daemon, every client window and
    ...                every ssh session to be ready.
    Start Cssh Cluster
    Assert Daemon Window Appears
    Assert Client Window Appears For Each Host
    Assert All Ssh Connections Established

Broadcast And Assert Every Host Received
    [Documentation]    Broadcast a unique message built from ${prefix} and assert every host
    ...                received it.
    [Arguments]    ${prefix}
    ${message}=    Unique Message    ${prefix}
    Focus Daemon And Broadcast    ${message}
    Wait Until Keyword Succeeds    10x    0.5s    Assert Every Host Received    ${message}

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
