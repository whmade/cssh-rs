*** Settings ***
Documentation       Windows E2E case: copying hostnames in control mode.
...                 Enters control mode, triggers the [h] copy command and asserts the
...                 system clipboard then holds the cluster's space-joined hostnames.

Resource            ../resources/cssh_rs_cluster.resource

Suite Setup         Run Keywords    Start Suite Recording    AND    Start Cssh Cluster
Suite Teardown      Run Keywords    Tear Down Cssh Cluster    AND    Stop Suite Recording


*** Variables ***
${CSSH_RS_BINARY}       %{CSSH_RS_BINARY}
@{ALIASES}              alpha    bravo
${CLUSTER_NAME}         e2e
${EXPECTED_HOSTNAMES}   alpha bravo


*** Test Cases ***
Copy Hostnames Puts Client Names On The Clipboard
    [Documentation]    Assert the [h] control-mode command copies the space-joined client
    ...                hostnames to the clipboard. A sentinel is staged first so a stale
    ...                clipboard cannot pass the assertion.
    Assert Daemon Window Appears
    Assert Client Window Appears For Each Host
    Set Clipboard    cssh-e2e-clipboard-sentinel
    Enter Control Mode
    Send Control Mode Key    h
    Wait Until Keyword Succeeds    10x    0.5s    Clipboard Should Equal    ${EXPECTED_HOSTNAMES}


*** Keywords ***
Clipboard Should Equal
    [Documentation]    Assert the system clipboard holds exactly ${expected}.
    [Arguments]    ${expected}
    ${actual}=    Get Clipboard
    Should Be Equal    ${actual}    ${expected}
