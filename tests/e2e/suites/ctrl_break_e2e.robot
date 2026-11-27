*** Settings ***
Documentation       Windows E2E cases for Ctrl+Break, each asserting one behaviour:
...                 - Ctrl+Break typed in a client window ends only that client's ssh
...                 - Ctrl+Break typed with the daemon focused ends the ssh of every enabled
...                 client and leaves the daemon running
...                 - Ctrl+Break typed at the daemon's hostname prompt is ignored
...                 Windows OpenSSH exits on Ctrl+Break, and a client closes once its ssh
...                 exits. Windows 10's inbox ConPTY predates win32-input-mode and drops the
...                 Ctrl+Break key, so the cases that need it to reach ssh skip on builds
...                 before Windows 11's 22000.

Resource            ../resources/cssh_rs_cluster.resource

Suite Setup         Start Cssh Cluster
Suite Teardown      Tear Down Cssh Cluster


*** Variables ***
${CSSH_RS_BINARY}       %{CSSH_RS_BINARY}
@{ALIASES}              alpha    bravo
@{ALL_ALIASES}          alpha    bravo    charlie
${CLUSTER_NAME}         e2e
${FIRST_HOST}           alpha
${SECOND_HOST}          bravo
${NEW_HOST}             charlie
${WIN32_INPUT_MODE_BUILD}    ${22000}


*** Test Cases ***
Ctrl Break At A Client Ends Only That Client
    [Documentation]    Press Ctrl+Break in the first client's window and assert its window
    ...                closes while the second client still receives daemon broadcasts.
    [Setup]    Skip Without Win32 Input Mode
    Assert All Ssh Connections Established
    Focus Window    @${FIRST_HOST}    substring
    Press Ctrl Break
    Wait Until Keyword Succeeds    20x    0.5s    Assert Client Window Gone    ${FIRST_HOST}
    ${message}=    Unique Message    SURVIVOR
    Focus Daemon And Broadcast    ${message}
    @{survivors}=    Create List    ${SECOND_HOST}
    Wait Until Keyword Succeeds    10x    0.5s    Assert Hosts Received    ${survivors}    ${message}

Ctrl Break From The Daemon Ends Every Enabled Client
    [Documentation]    Disable the second client, press Ctrl+Break with the daemon focused,
    ...                and assert the first client's window closes while the daemon stays up
    ...                and the disabled client's ssh still takes typed input. A daemon that
    ...                Ctrl+Break ended would close its own window, and the clients with it.
    [Setup]    Run Keywords    Skip Without Win32 Input Mode    AND    Restart Cssh Cluster
    Assert All Ssh Connections Established
    Disable Second Client
    Focus Window    ${DAEMON_TITLE}
    Press Ctrl Break
    Wait Until Keyword Succeeds    20x    0.5s    Assert Client Window Gone    ${FIRST_HOST}
    Assert Daemon Window Appears
    ${message}=    Unique Message    SURVIVOR
    Focus Client And Type    ${SECOND_HOST}    ${message}
    @{survivors}=    Create List    ${SECOND_HOST}
    Wait Until Keyword Succeeds    10x    0.5s    Assert Hosts Received    ${survivors}    ${message}

Ctrl Break At The Hostname Prompt Is Ignored
    [Documentation]    Press Ctrl+Break at the control-mode [c] hostname prompt, then type
    ...                ${NEW_HOST}, and assert its client window opens. A daemon that
    ...                Ctrl+Break ended, or a prompt it cancelled, would never launch it.
    [Setup]    Restart Cssh Cluster    ${ALIASES}    ${ALL_ALIASES}
    Assert All Ssh Connections Established
    Enter Control Mode
    Send Control Mode Key    c
    Press Ctrl Break
    Type Line    ${NEW_HOST}
    Wait Until Keyword Succeeds    15x    1s    Focus Window    @${NEW_HOST}    substring


*** Keywords ***
Skip Without Win32 Input Mode
    [Documentation]    Skip the test where the inbox ConPTY drops the Ctrl+Break key.
    ${build}=    Evaluate    sys.getwindowsversion().build    modules=sys
    Skip If    ${build} < ${WIN32_INPUT_MODE_BUILD}
    ...    Windows build ${build} has no ConPTY win32-input-mode to carry Ctrl+Break

Press Ctrl Break
    [Documentation]    Press Ctrl+Break as a keyboard sends it, left Ctrl with the Break key.
    Press Keys With Scan Codes    ctrlleft    break
