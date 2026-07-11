*** Settings ***
Documentation       Windows E2E cases: copying hostnames in control mode reflects live
...                 cluster membership. The cases run in order against one shared cluster and
...                 build on each other - the first adds a spare host with the control-mode [c]
...                 command and asserts a daemon broadcast reaches it, the second closes an
...                 original client and asserts the [h] copy leaves only the still-open
...                 hostnames (including the added host) on the clipboard. The split is only so
...                 each behavior is a named segment in the recording, not to isolate the cases.

Resource            ../resources/cssh_rs_cluster.resource

Suite Setup         Start Cluster And Await Readiness
Suite Teardown      Tear Down Cssh Cluster


*** Variables ***
${CSSH_RS_BINARY}       %{CSSH_RS_BINARY}
@{ALIASES}              alpha    bravo
${NEW_HOST}             charlie
${CLOSED_HOST}          bravo
@{ALL_ALIASES}          alpha    bravo    charlie
${CLUSTER_NAME}         e2e
${EXPECTED_HOSTNAMES}   alpha charlie


*** Test Cases ***
Added Client Joins Broadcast Targeting
    [Documentation]    Add ${NEW_HOST} at runtime and assert a daemon-focused broadcast reaches
    ...                every host including the newly added one.
    Add Host    ${NEW_HOST}
    Wait Until Keyword Succeeds    15x    1s    Focus Window    @${NEW_HOST}    substring
    Wait Until Keyword Succeeds    30x    1s    All Connections Reported    3
    ${message}=    Unique Message    ADDHOST
    Focus Daemon And Broadcast    ${message}
    Wait Until Keyword Succeeds    10x    0.5s    Assert Hosts Received    ${ALL_ALIASES}    ${message}

Copy Hostnames Reflects Live Membership
    [Documentation]    Close ${CLOSED_HOST} and assert the [h] copy leaves only the still-open
    ...                hostnames - the surviving original plus ${NEW_HOST}, added by the
    ...                previous case - on the clipboard.
    Close Client    ${CLOSED_HOST}
    Set Clipboard    cssh-e2e-clipboard-sentinel
    Enter Control Mode
    Send Control Mode Key    h
    Wait Until Keyword Succeeds    10x    0.5s    Clipboard Should Equal    ${EXPECTED_HOSTNAMES}


*** Keywords ***
Start Cluster And Await Readiness
    [Documentation]    Start the cluster with a spare host available to add at runtime and wait
    ...                for the daemon, every client window and every ssh session to be ready.
    Start Cssh Cluster    ${ALIASES}    ${ALL_ALIASES}
    Assert Daemon Window Appears
    Assert Client Window Appears For Each Host
    Assert All Ssh Connections Established

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
