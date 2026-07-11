*** Settings ***
Documentation       Windows E2E cases for cssh-rs, each asserting one behaviour:
...                 - launch brings up the daemon window and one window per host
...                 - input typed with the daemon focused broadcasts to every host
...                 - input typed with one client focused reaches only that host
...                 - closing the daemon closes every client window
...                 - closing every client closes the daemon
...                 - teardown stops cssh-rs and sshd and removes the temp tree

Resource            ../resources/cssh_rs_cluster.resource

Suite Setup         Run Keywords    Start Suite Recording    AND    Start Cssh Cluster
Suite Teardown      Run Keywords    Tear Down Cssh Cluster    AND    Stop Suite Recording


*** Variables ***
${CSSH_RS_BINARY}       %{CSSH_RS_BINARY}
@{ALIASES}              alpha    bravo
${CLUSTER_NAME}         e2e


*** Test Cases ***
Cluster Launch Brings Up Daemon And Client Windows
    Assert Daemon Window Appears
    Assert Client Window Appears For Each Host

Broadcast Reaches Every Host When Daemon Focused
    Assert All Ssh Connections Established
    ${message}=    Unique Message    BROADCAST
    Focus Daemon And Broadcast    ${message}
    Wait Until Keyword Succeeds    10x    0.5s    Assert Every Host Received    ${message}

Focused Client Receives Input Alone
    Assert All Ssh Connections Established
    ${message}=    Unique Message    GATED
    ${target}=    Set Variable    ${ALIASES}[0]
    Focus Client And Type    ${target}    ${message}
    Wait Until Keyword Succeeds    10x    0.5s    Assert Only Host Received    ${target}    ${message}

Closing The Daemon Closes Every Client
    Assert All Ssh Connections Established
    Close Daemon Window
    Wait Until Keyword Succeeds    15x    1s    Assert Daemon Window Gone
    FOR    ${alias}    IN    @{ALIASES}
        Wait Until Keyword Succeeds    15x    1s    Assert Client Window Gone    ${alias}
    END

Closing Every Client Closes The Daemon
    [Setup]    Restart Cssh Cluster
    Assert All Ssh Connections Established
    Close All Client Windows
    Wait Until Keyword Succeeds    15x    1s    Assert Daemon Window Gone
