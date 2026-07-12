*** Settings ***
Documentation       Record the cssh-rs demo GIF - the minimal broadcast cut.
...
...                 This is a Robot task, not a test: it drives real console
...                 windows, records the desktop and exports a GIF. It asserts
...                 nothing - a failure means the demo could not be recorded.
...                 The xtask passes ${BINARY}, ${OUTPUT_DIR} and ${GIF} as
...                 --variable overrides. Windows only.

Library             cssh_rs_demo.recorder.DemoRecorder

Suite Teardown      Tear Down Demo


*** Variables ***
${BINARY}           ${EMPTY}
${OUTPUT_DIR}       target/demo
${GIF}              target/demo/cssh-rs.gif
${FPS}              ${10}
@{HOSTS}            web01    web02    db01


*** Tasks ***
Record The Broadcast Demo
    Start Demo    ${BINARY}    ${OUTPUT_DIR}    ${HOSTS}    fps=${FPS}
    Wait For Hosts
    Hold    3

    Broadcast    echo hello from the cssh-rs cluster
    Hold    1

    # Ctrl+C closes every client (and then the daemon) so the clip shows the
    # cluster tearing down; keep recording a beat longer to capture it.
    Focus Daemon
    Send Hotkey    ctrl    c
    Hold    0.1

    Export Demo Gif    ${GIF}    fps=${FPS}
