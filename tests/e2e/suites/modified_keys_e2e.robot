*** Settings ***
Documentation       Windows E2E cases for navigation, function and editing keys, alone or
...                 with Ctrl, Shift and Alt, and for chords, each asserting one behaviour:
...                 - Ctrl+Left reaches every host with the Ctrl modifier parameter
...                 - Shift+F5 reaches every host with the Shift modifier parameter
...                 - Shift+F1 reaches every host as a CSI sequence instead of SS3
...                 - Alt+Left reaches every host with Alt in the modifier parameter
...                   instead of an ESC prefix
...                 - Ctrl+Alt+F5 reaches every host with both modifiers in the
...                   parameter instead of being taken for AltGr
...                 - Backspace reaches every host as DEL, not the BS the console composes
...                 - Ctrl+Backspace reaches every host as BS
...                 - Alt+Backspace reaches every host as ESC DEL
...                 - Shift+Tab reaches every host as CBT instead of a plain tab
...                 - Ctrl+Enter reaches every host as LF
...                 - Ctrl+Tab, Ctrl+Shift+Tab, Ctrl+Shift+Enter and Ctrl+Shift+Backspace,
...                   which compose no character, reach every host as a tab, CBT, LF and BS
...                 - Backspace typed at a client reaches its host as DEL
...                 - Ctrl+Left, Shift+F5 and Alt+Left typed at a client reach its host
...                   with their modifier parameters
...                 - Ctrl+Alt+F5 typed at a client reaches its host with both modifiers
...                   in the parameter instead of being taken for AltGr
...                 - Alt+X typed at a client reaches its host as ESC x

Resource            ../resources/raw_input_cluster.resource

Suite Setup         Start Raw Input Cluster
Suite Teardown      Tear Down Cssh Cluster


*** Variables ***
${CSSH_RS_BINARY}       %{CSSH_RS_BINARY}
@{ALIASES}              alpha    bravo
${CLIENT}               alpha
${CLUSTER_NAME}         e2e


*** Test Cases ***
Ctrl Left Arrives With The Ctrl Modifier Parameter
    Keys Typed In Daemon Reach Every Host As    \x1b[1;5D    ctrlleft    left

Shift F5 Arrives With The Shift Modifier Parameter
    Keys Typed In Daemon Reach Every Host As    \x1b[15;2~    shiftleft    f5

Shift F1 Arrives As A CSI Sequence
    Keys Typed In Daemon Reach Every Host As    \x1b[1;2P    shiftleft    f1

Alt Left Arrives With Alt In The Modifier Parameter
    Keys Typed In Daemon Reach Every Host As    \x1b[1;3D    altleft    left

Ctrl Alt F5 Arrives With Both Modifiers In The Parameter
    Keys Typed In Daemon Reach Every Host As    \x1b[15;7~    ctrlleft    altleft    f5

Backspace Arrives As DEL
    Keys Typed In Daemon Reach Every Host As    \x7f    backspace

Ctrl Backspace Arrives As BS
    Keys Typed In Daemon Reach Every Host As    \x08    ctrlleft    backspace

Alt Backspace Arrives As ESC DEL
    Keys Typed In Daemon Reach Every Host As    \x1b\x7f    altleft    backspace

Shift Tab Arrives As CBT
    Keys Typed In Daemon Reach Every Host As    \x1b[Z    shiftleft    tab

Ctrl Enter Arrives As LF
    Keys Typed In Daemon Reach Every Host As    \n    ctrlleft    enter

Ctrl Tab Arrives As A Tab
    Keys Typed In Daemon Reach Every Host As    \t    ctrlleft    tab

Ctrl Shift Tab Arrives As CBT
    Keys Typed In Daemon Reach Every Host As    \x1b[Z    ctrlleft    shiftleft    tab

Ctrl Shift Enter Arrives As LF
    Keys Typed In Daemon Reach Every Host As    \n    ctrlleft    shiftleft    enter

Ctrl Shift Backspace Arrives As BS
    Keys Typed In Daemon Reach Every Host As    \x08    ctrlleft    shiftleft    backspace

Backspace At A Client Arrives As DEL
    Keys Typed At A Client Reach Its Host As    ${CLIENT}    \x7f    backspace

Ctrl Left At A Client Arrives With The Ctrl Modifier Parameter
    Keys Typed At A Client Reach Its Host As    ${CLIENT}    \x1b[1;5D    ctrlleft    left

Shift F5 At A Client Arrives With The Shift Modifier Parameter
    Keys Typed At A Client Reach Its Host As    ${CLIENT}    \x1b[15;2~    shiftleft    f5

Alt Left At A Client Arrives With Alt In The Modifier Parameter
    Keys Typed At A Client Reach Its Host As    ${CLIENT}    \x1b[1;3D    altleft    left

Ctrl Alt F5 At A Client Arrives With Both Modifiers In The Parameter
    Keys Typed At A Client Reach Its Host As    ${CLIENT}    \x1b[15;7~    ctrlleft    altleft    f5

Alt X At A Client Arrives As ESC X
    Keys Typed At A Client Reach Its Host As    ${CLIENT}    \x1bx    altleft    x
