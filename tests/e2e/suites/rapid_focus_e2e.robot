*** Settings ***
Documentation       Windows E2E case: rapidly refocusing the daemon raises all clients.
...                 Opens a large cluster and a neutral window, switches focus between the
...                 neutral window and the daemon several times in quick succession, then
...                 asserts every client window ends stacked above the neutral window - the
...                 daemon's z-order sync raised them all rather than leaving any covered.

Resource            ../resources/cssh_rs_cluster.resource

Suite Setup         Start Cssh Cluster
Suite Teardown      Tear Down Rapid Focus Suite


*** Variables ***
${CSSH_RS_BINARY}       %{CSSH_RS_BINARY}
@{ALIASES}              h01    h02    h03    h04    h05    h06    h07    h08    h09    h10
${CLUSTER_NAME}         e2e
${OTHER_WINDOW_TITLE}   Notepad
${FOCUS_SWITCHES}       ${5}
${FOCUS_DWELL}          250ms


*** Test Cases ***
Rapidly Refocusing The Daemon Raises Every Client
    [Documentation]    After repeatedly switching focus between a neutral window and the
    ...                daemon, every one of the ten client windows must sit above the neutral
    ...                window in the z-order. Switches are paced at human speed, not machine
    ...                speed, and begin only once every client has fully connected.
    Assert Daemon Window Appears
    Assert Client Window Appears For Each Host
    # Switch only after every client has connected, not merely once its window appeared.
    Assert All Ssh Connections Established
    Start Other Window
    FOR    ${switch}    IN RANGE    ${FOCUS_SWITCHES}
        # Pace at human speed; back-to-back machine-fast switches never happen in real use.
        Sleep    ${FOCUS_DWELL}
        Focus Window    ${OTHER_WINDOW_TITLE}    substring
        # Pace at human speed; back-to-back machine-fast switches never happen in real use.
        Sleep    ${FOCUS_DWELL}
        Focus Window    ${DAEMON_TITLE}
    END
    Wait Until Keyword Succeeds    10x    1s
    ...    Assert All Client Windows Raised Above    ${OTHER_WINDOW_TITLE}


*** Keywords ***
Start Other Window
    [Documentation]    Launch a neutral foreground window (Notepad) to switch focus to.
    Start Process    notepad.exe
    Wait Until Keyword Succeeds    15x    1s    Focus Window    ${OTHER_WINDOW_TITLE}    substring

Tear Down Rapid Focus Suite
    [Documentation]    Close Notepad, then run the standard cluster teardown.
    Run Keyword And Ignore Error    Run Process    taskkill    /F    /IM    notepad.exe
    Tear Down Cssh Cluster
