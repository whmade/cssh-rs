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

    Show Chapter    cssh-rs: type once, run everywhere
    Broadcast    echo hello from the cssh-rs cluster
    Hold    3

    Export Demo Gif    ${GIF}    fps=${FPS}
