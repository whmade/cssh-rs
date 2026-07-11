*** Settings ***
Documentation       Windows E2E cases for cssh-rs, each asserting one behaviour:
...                 - launch brings up the daemon window and one window per host
...                 - input typed with the daemon focused broadcasts to every host
...                 - input typed with one client focused reaches only that host
...                 - a large paste broadcasts to every host without killing a client
...                 - closing the daemon closes every client window
...                 - closing every client closes the daemon
...                 - teardown stops cssh-rs and sshd and removes the temp tree

Resource            ../resources/cssh_rs_cluster.resource

Suite Setup         Start Cssh Cluster
Suite Teardown      Tear Down Cssh Cluster


*** Variables ***
${CSSH_RS_BINARY}       %{CSSH_RS_BINARY}
@{ALIASES}              alpha    bravo
${CLUSTER_NAME}         e2e
${PAYLOAD_LENGTH}       ${10000}


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

Large Paste Reaches Every Host Without Killing A Client
    Assert All Ssh Connections Established
    ${payload}=    Large Payload
    Paste Into Daemon    ${payload}
    Wait Until Keyword Succeeds    30x    1s    Assert Every Host Received    ${payload}
    Assert All Client Windows Present
    # Let the pasted line finish rendering before the next test closes the daemon.
    Sleep    2s

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


*** Keywords ***
Large Payload
    [Documentation]    Return a ${PAYLOAD_LENGTH}-character line prefixed with a unique token
    ...                so the marker assertion cannot match leftover output.
    ${prefix}=    Unique Message    PASTE
    ${payload}=    Evaluate    $prefix + "x" * ($PAYLOAD_LENGTH - len($prefix))
    RETURN    ${payload}

Paste Into Daemon
    [Documentation]    Stage ${payload} on the clipboard and paste it into the daemon, then
    ...                press Enter to flush the line to every marker.
    [Arguments]    ${payload}
    Set Clipboard    ${payload}
    Focus Window    ${DAEMON_TITLE}
    # conhost pastes the clipboard on a right-click (QuickEdit mode); it has no Ctrl+V paste.
    Right Click Window    ${DAEMON_TITLE}
    Press Key    enter

Assert All Client Windows Present
    [Documentation]    Assert each client window still exists after the paste - a client that
    ...                died on the pipe would be gone.
    FOR    ${alias}    IN    @{ALIASES}
        Focus Window    @${alias}    substring
    END
