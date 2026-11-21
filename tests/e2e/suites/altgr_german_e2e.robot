*** Settings ***
Documentation       Windows E2E cases for AltGr input on the German layout, each asserting
...                 one behaviour:
...                 - AltGr characters typed in the daemon reach every host as typed
...                 - left Ctrl + left Alt types the same character as AltGr
...                 - a paste holding AltGr and keyless characters reaches every host
...                 - AltGr+A, which types nothing on German, does not open control mode
...                 - a held AltGr key types one character per auto-repeat
...                 - right Ctrl held with AltGr sends Ctrl plus the AltGr character, in
...                   either press order
...                 - left Alt held with AltGr sends Alt plus the AltGr character
...                 - AltGr characters typed at a focused client reach only that host
...                 - Alt+X and Ctrl+B, which compose no AltGr character, stay chords
...
...                 Letters and digits are typed through pyautogui's virtual keys, which
...                 German maps to the same letters and digits; everything with AltGr is
...                 typed with real scan codes. Keys are named by their US characters:
...                 ``-`` is sharp-s, ``]`` is plus and ``nonusbackslash`` is less-than.

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
${OPENING_BRACE}            {
${AT_SIGN}                  @
# Characters German has no key for; the console pastes them without modifiers.
${KEYLESS_CHARACTERS}       éá


*** Test Cases ***
Typed AltGr Characters Reach Every Host
    [Documentation]    AltGr+E, 7, 8, 9, 0, Q, sharp-s, plus and less-than reach every host
    ...                as their characters, without the ESC of Alt or the control byte of
    ...                Ctrl.
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
    [Documentation]    Windows treats left Ctrl + left Alt as AltGr, so left Ctrl + left Alt
    ...                + Q reaches every host as the AltGr character of Q.
    Keys Typed In Daemon Reach Every Host As    ${AT_SIGN}    ctrlleft    altleft    q

Pasted AltGr And Keyless Characters Reach Every Host
    [Documentation]    A paste reaches every host unchanged, both the characters the console
    ...                presses AltGr for and those German has no key for.
    ${prefix}=    Unique Message    PASTE
    ${line}=    Set Variable    ${prefix}${EURO}${OPENING_BRACE}${KEYLESS_CHARACTERS}
    Paste Into Daemon    ${line}
    Wait Until Keyword Succeeds    10x    0.5s    Assert Every Host Received    ${line}

AltGr A Without A Character Does Not Enter Control Mode
    [Documentation]    AltGr+A types nothing on German and must not open control mode, which
    ...                would swallow the rest of the line as control-mode commands.
    Keys Typed In Daemon Reach Every Host As    ${EMPTY}    altgr    a

Held AltGr Key Types One Character Per Auto-Repeat
    [Documentation]    AltGr+E held down for five key-downs reaches every host as five euro
    ...                signs.
    ${prefix}=    Unique Message    REPEAT
    Focus Window    ${DAEMON_TITLE}
    Type Text    ${prefix}
    Press Keys With Scan Codes    altgr    e    repeat=5
    Press Key    enter
    Wait Until Keyword Succeeds    10x    0.5s
    ...    Assert Every Host Received    ${prefix}${{ $EURO * 5 }}

Right Ctrl Then AltGr Sends Ctrl Backslash
    [Documentation]    Right Ctrl pressed before AltGr keeps Ctrl, so AltGr+sharp-s arrives
    ...                as Ctrl+\\ (0x1c) rather than as a backslash.
    Keys Typed In Daemon Reach Every Host As    \x1c    ctrlright    altgr    -

AltGr Then Right Ctrl Sends Ctrl Backslash
    [Documentation]    Right Ctrl pressed after AltGr keeps Ctrl too, so the press order does
    ...                not matter.
    Keys Typed In Daemon Reach Every Host As    \x1c    altgr    ctrlright    -

Right Ctrl With AltGr Q Sends Ctrl At
    [Documentation]    Right Ctrl + AltGr + Q arrives as Ctrl+@ (0x00).
    Keys Typed In Daemon Reach Every Host As    \x00    ctrlright    altgr    q

Right Ctrl With AltGr 9 Sends Ctrl Right Bracket
    [Documentation]    Right Ctrl + AltGr + 9 arrives as Ctrl+] (0x1d).
    Keys Typed In Daemon Reach Every Host As    \x1d    ctrlright    altgr    9

Left Alt With AltGr Sends Alt Plus The AltGr Character
    [Documentation]    Left Alt held with AltGr keeps Alt, so AltGr+E arrives as ESC plus the
    ...                euro sign.
    Keys Typed In Daemon Reach Every Host As    \x1b${EURO}    altleft    altgr    e

Alt X Arrives As ESC X
    [Documentation]    Alt+X keeps Alt, which terminals send as an ESC prefix.
    Keys Typed In Daemon Reach Every Host As    \x1bx    altleft    x

Ctrl B Arrives As Its Control Byte
    [Documentation]    Ctrl+B keeps Ctrl and arrives as 0x02.
    Keys Typed In Daemon Reach Every Host As    \x02    ctrlleft    b

AltGr Characters Typed At A Client Reach Only That Host
    [Documentation]    AltGr characters typed at a focused client reach that client's host as
    ...                typed and no other host.
    ${prefix}=    Unique Message    CLIENT
    Focus Window    @${CLIENT}    substring
    Type Text    ${prefix}
    Press Keys With Scan Codes    altgr    e
    Press Keys With Scan Codes    altgr    q
    Press Key    enter
    Wait Until Keyword Succeeds    10x    0.5s
    ...    Assert Only Host Received    ${CLIENT}    ${prefix}${EURO}${AT_SIGN}
