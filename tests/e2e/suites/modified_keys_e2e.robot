*** Settings ***
Documentation       Windows E2E cases for Ctrl, Shift and Alt held with navigation and
...                 function keys in the daemon, each asserting one behaviour:
...                 - Ctrl+Left reaches every host with the Ctrl modifier parameter
...                 - Shift+F5 reaches every host with the Shift modifier parameter
...                 - Shift+F1 reaches every host as a CSI sequence instead of SS3
...                 - Alt+Left reaches every host with Alt in the modifier parameter
...                   instead of an ESC prefix
...                 - Ctrl+Alt+F5 reaches every host with both modifiers in the
...                   parameter instead of being taken for AltGr
...
...                 No case types at a client, which drops modifiers until issue #308.

Resource            ../resources/raw_input_cluster.resource

Suite Setup         Start Raw Input Cluster
Suite Teardown      Tear Down Cssh Cluster


*** Variables ***
${CSSH_RS_BINARY}       %{CSSH_RS_BINARY}
@{ALIASES}              alpha    bravo
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
