*** Settings ***
Documentation       Windows E2E case: copying hostnames in control mode reflects live
...                 cluster membership. Adds a spare host with the control-mode [c] command
...                 and asserts a daemon broadcast reaches it like the rest, then closes one
...                 original client and asserts the [h] copy command puts only the still-open
...                 hostnames on the clipboard.

Resource            ../resources/cssh_rs_cluster.resource

Suite Setup         Run Keywords    Start Suite Recording
...                     AND    Start Cssh Cluster    ${ALIASES}    ${ALL_ALIASES}
Suite Teardown      Run Keywords    Tear Down Cssh Cluster    AND    Stop Suite Recording


*** Variables ***
${CSSH_RS_BINARY}       %{CSSH_RS_BINARY}
@{ALIASES}              alpha    bravo
${NEW_HOST}             charlie
${CLOSED_HOST}          bravo
@{ALL_ALIASES}          alpha    bravo    charlie
${CLUSTER_NAME}         e2e
${EXPECTED_HOSTNAMES}   alpha charlie


*** Test Cases ***
Copy Hostnames Reflects Added And Closed Clients
    [Documentation]    Add ${NEW_HOST} at runtime and assert a daemon-focused broadcast
    ...                reaches every host including it. Then close ${CLOSED_HOST} and assert
    ...                the [h] copy command leaves only the still-open hostnames on the
    ...                clipboard.
    Assert Daemon Window Appears
    Assert Client Window Appears For Each Host
    Wait Until Keyword Succeeds    30x    1s    All Connections Reported    2
    Add Host    ${NEW_HOST}
    Wait Until Keyword Succeeds    15x    1s    Focus Window    @${NEW_HOST}    substring
    Wait Until Keyword Succeeds    30x    1s    All Connections Reported    3
    ${message}=    Unique Message    ADDHOST
    Focus Daemon And Broadcast    ${message}
    Wait Until Keyword Succeeds    10x    0.5s    Assert Hosts Received    ${ALL_ALIASES}    ${message}
    Close Client    ${CLOSED_HOST}
    Set Clipboard    cssh-e2e-clipboard-sentinel
    Enter Control Mode
    Send Control Mode Key    h
    Wait Until Keyword Succeeds    10x    0.5s    Clipboard Should Equal    ${EXPECTED_HOSTNAMES}


*** Keywords ***
Add Host
    [Documentation]    Enter control mode, trigger the [c] add-host prompt and submit
    ...                ${hostname} so cssh-rs launches an additional client for it.
    [Arguments]    ${hostname}
    Enter Control Mode
    Send Control Mode Key    c
    Type Line    ${hostname}

Close Client
    [Documentation]    Close the ${alias} client window and wait until it is gone, so its
    ...                process exits and the daemon drops it from the active client list.
    [Arguments]    ${alias}
    Close Window    @${alias}    substring

Clipboard Should Equal
    [Documentation]    Assert the system clipboard holds exactly ${expected}.
    [Arguments]    ${expected}
    ${actual}=    Get Clipboard
    Should Be Equal    ${actual}    ${expected}
