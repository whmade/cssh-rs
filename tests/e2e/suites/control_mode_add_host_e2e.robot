*** Settings ***
Documentation       Windows E2E case: adding a host in control mode.
...                 Launches a two-host cluster while sshd also serves a spare host,
...                 adds that host with the control-mode [c] command, and asserts a
...                 daemon broadcast then reaches the new host exactly like the rest.

Resource            ../resources/cssh_rs_cluster.resource

Suite Setup         Run Keywords    Start Suite Recording
...                     AND    Start Cssh Cluster    ${CLUSTER_ALIASES}    ${ALL_ALIASES}
Suite Teardown      Run Keywords    Tear Down Cssh Cluster    AND    Stop Suite Recording


*** Variables ***
${CSSH_RS_BINARY}       %{CSSH_RS_BINARY}
@{CLUSTER_ALIASES}      alpha    bravo
${NEW_HOST}             charlie
@{ALL_ALIASES}          alpha    bravo    charlie
${CLUSTER_NAME}         e2e


*** Test Cases ***
Added Host Receives Broadcast Like The Rest
    [Documentation]    Add ${NEW_HOST} at runtime, then assert a daemon-focused broadcast
    ...                reaches every host including the newly added one.
    Assert Daemon Window Appears
    Wait Until Keyword Succeeds    30x    1s    All Connections Reported    2
    Add Host    ${NEW_HOST}
    Wait Until Keyword Succeeds    15x    1s    Focus Window    @${NEW_HOST}    substring
    Wait Until Keyword Succeeds    30x    1s    All Connections Reported    3
    ${message}=    Unique Message    ADDHOST
    Focus Daemon And Broadcast    ${message}
    Wait Until Keyword Succeeds    10x    0.5s    Assert Hosts Received    ${ALL_ALIASES}    ${message}


*** Keywords ***
Add Host
    [Documentation]    Enter control mode, trigger the [c] add-host prompt and submit
    ...                ${hostname} so cssh-rs launches an additional client for it.
    [Arguments]    ${hostname}
    Enter Control Mode
    Send Control Mode Key    c
    Type Line    ${hostname}
