*** Settings ***
Documentation       Windows E2E cases for AltGr input on the German layout, each asserting
...                 one behaviour:
...                 - AltGr characters typed in the daemon reach every host as typed
...                 - left Ctrl + left Alt types the same character as AltGr
...                 - a paste holding AltGr and keyless characters, an emoji among them,
...                   reaches every host
...                 - AltGr+A, which types nothing on German, does not open control mode
...                 - a held AltGr key types one character per auto-repeat
...                 - right Ctrl held with AltGr sends Ctrl plus the AltGr character, in
...                   either press order
...                 - left Alt held with AltGr sends Alt plus the AltGr character
...                 - AltGr characters typed at a focused client reach only that host
...                 - a paste holding AltGr and keyless characters, an emoji among them,
...                   into a focused client reaches only that host
...                 - Alt+X and Ctrl+B, which compose no AltGr character, stay chords
...
...                 Type Text stays correct on German, which maps pyautogui's virtual keys
...                 for letters and digits to the same characters as US. Scan code keys are
...                 named by their US characters: ``-`` is sharp-s, ``]`` is plus and
...                 ``nonusbackslash`` is less-than.

Resource            ../resources/altgr_layout.resource

Suite Setup         Start Cluster On Layout    ${GERMAN_LAYOUT}
Suite Teardown      Tear Down Cluster And Restore Layout


*** Variables ***
${CSSH_RS_BINARY}           %{CSSH_RS_BINARY}
@{ALIASES}                  alpha    bravo
${CLIENT}                   alpha
${CLUSTER_NAME}             e2e
${GERMAN_LAYOUT}            00000407
# What German types for AltGr+E, 7, 8, 9, 0, Q, sharp-s, plus and less-than.
${ALTGR_CHARACTERS}         €{[]}@\\~|
${EURO}                     €
${AT_SIGN}                  @
# Characters German has no key for: letters, pasted without modifiers, and a surrogate-pair emoji.
${KEYLESS_CHARACTERS}       éá\U0001F600


*** Test Cases ***
Typed AltGr Characters Reach Every Host
    ${prefix}=    Unique Message    ALTGR
    Focus Window    ${DAEMON_TITLE}
    Type Text    ${prefix}
    FOR    ${key}    IN    e    7    8    9    0    q    -    ]    nonusbackslash
        Press Keys With Scan Codes    altgr    ${key}
    END
    Press Key    enter
    Wait Until Keyword Succeeds    10x    0.5s
    ...    Assert Every Host Received    ${prefix}${ALTGR_CHARACTERS}

Left Ctrl And Left Alt Type The AltGr Character
    [Documentation]    Windows treats left Ctrl + left Alt as AltGr.
    Keys Typed In Daemon Reach Every Host As    ${AT_SIGN}    ctrlleft    altleft    q

Pasted AltGr And Keyless Characters Reach Every Host
    ${prefix}=    Unique Message    PASTE
    ${line}=    Set Variable    ${prefix}${ALTGR_CHARACTERS}${KEYLESS_CHARACTERS}
    Paste Into Daemon    ${line}
    Wait Until Keyword Succeeds    10x    0.5s    Assert Every Host Received    ${line}

AltGr A Without A Character Does Not Enter Control Mode
    [Documentation]    AltGr+A types nothing on German, and control mode would swallow the
    ...                text typed after it.
    Keys Typed In Daemon Reach Every Host As    ${EMPTY}    altgr    a

Held AltGr Key Types One Character Per Auto-Repeat
    ${prefix}=    Unique Message    REPEAT
    Focus Window    ${DAEMON_TITLE}
    Type Text    ${prefix}
    Press Keys With Scan Codes    altgr    e    repeat=5
    Press Key    enter
    Wait Until Keyword Succeeds    10x    0.5s
    ...    Assert Every Host Received    ${prefix}${{ $EURO * 5 }}

Right Ctrl Then AltGr Sends Ctrl Backslash
    Keys Typed In Daemon Reach Every Host As    \x1c    ctrlright    altgr    -

AltGr Then Right Ctrl Sends Ctrl Backslash
    Keys Typed In Daemon Reach Every Host As    \x1c    altgr    ctrlright    -

Right Ctrl With AltGr Q Sends Ctrl At
    Keys Typed In Daemon Reach Every Host As    \x00    ctrlright    altgr    q

Right Ctrl With AltGr 9 Sends Ctrl Right Bracket
    Keys Typed In Daemon Reach Every Host As    \x1d    ctrlright    altgr    9

Left Alt With AltGr Sends Alt Plus The AltGr Character
    Keys Typed In Daemon Reach Every Host As    \x1b${EURO}    altleft    altgr    e

Alt X Arrives As ESC X
    Keys Typed In Daemon Reach Every Host As    \x1bx    altleft    x

Ctrl B Arrives As Its Control Byte
    [Documentation]    Not Ctrl+C, whose 0x03 ends the marker writer.
    Keys Typed In Daemon Reach Every Host As    \x02    ctrlleft    b

AltGr Characters Typed At A Client Reach Only That Host
    ${prefix}=    Unique Message    CLIENT
    Focus Window    @${CLIENT}    substring
    Type Text    ${prefix}
    Press Keys With Scan Codes    altgr    e
    Press Keys With Scan Codes    altgr    q
    Press Key    enter
    Wait Until Keyword Succeeds    10x    0.5s
    ...    Assert Only Host Received    ${CLIENT}    ${prefix}${EURO}${AT_SIGN}

Pasted AltGr And Keyless Characters At A Client Reach Only That Host
    ${prefix}=    Unique Message    CLIENTPASTE
    ${line}=    Set Variable    ${prefix}${ALTGR_CHARACTERS}${KEYLESS_CHARACTERS}
    Paste Into Client    ${CLIENT}    ${line}
    Wait Until Keyword Succeeds    10x    0.5s    Assert Only Host Received    ${CLIENT}    ${line}
