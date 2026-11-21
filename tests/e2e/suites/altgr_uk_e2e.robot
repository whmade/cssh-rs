*** Settings ***
Documentation       Windows E2E cases for AltGr input in the daemon on the United Kingdom
...                 layout, each asserting one behaviour:
...                 - AltGr characters typed in the daemon reach every host as typed
...                 - a Shift+AltGr character typed in the daemon reaches every host as typed
...                 - AltGr+A typed in the daemon types its character, not control mode
...                 - a multi-line paste holding AltGr characters reaches every host whole
...
...                 Type Text stays correct on UK, which maps pyautogui's virtual keys for
...                 letters and digits to the same characters as US. Unlike German, UK has
...                 a character on AltGr+A, which used to open control mode.

Resource            ../resources/altgr_layout.resource

Suite Setup         Start Cluster On Layout    ${UK_LAYOUT}
Suite Teardown      Tear Down Cluster And Restore Layout


*** Variables ***
${CSSH_RS_BINARY}       %{CSSH_RS_BINARY}
@{ALIASES}              alpha    bravo
${CLUSTER_NAME}         e2e
${UK_LAYOUT}            00000809
# What the UK layout types for AltGr+4, AltGr+E, Shift+AltGr+E and AltGr+A.
${EURO}                 €
${E_ACUTE}              é
${CAPITAL_E_ACUTE}      É
${A_ACUTE}              á


*** Test Cases ***
Typed AltGr Characters Reach Every Host
    ${prefix}=    Unique Message    ALTGR
    Focus Window    ${DAEMON_TITLE}
    Type Text    ${prefix}
    Press Keys With Scan Codes    altgr    4
    Press Keys With Scan Codes    altgr    e
    Press Key    enter
    Wait Until Keyword Succeeds    10x    0.5s
    ...    Assert Every Host Received    ${prefix}${EURO}${E_ACUTE}

Typed Shift AltGr Character Reaches Every Host
    Keys Typed In Daemon Reach Every Host As    ${CAPITAL_E_ACUTE}    shiftleft    altgr    e

Typed AltGr A Does Not Enter Control Mode
    Keys Typed In Daemon Reach Every Host As    ${A_ACUTE}    altgr    a

Pasted AltGr Characters Reach Every Host
    [Documentation]    The second line proves the paste went on past AltGr+A.
    ${first}=    Unique Message    PASTEALTGR
    ${first}=    Set Variable    ${first}${A_ACUTE}${EURO}${E_ACUTE}
    ${second}=    Unique Message    PASTEREST
    Paste Into Daemon    ${first}\r\n${second}
    Wait Until Keyword Succeeds    10x    0.5s    Assert Every Host Received    ${first}
    Wait Until Keyword Succeeds    10x    0.5s    Assert Every Host Received    ${second}
