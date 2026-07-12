*** Settings ***
Documentation       Record the cssh-rs demo GIF - the broadcast cut.
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


*** Tasks ***
Record The Broadcast Demo
    Start Demo    ${BINARY}    ${OUTPUT_DIR}    fps=${FPS}
    Wait For Hosts

    Broadcast    cd demo/data
    Broadcast    ll
    Sleep    2s

    Export Demo Gif    ${GIF}    fps=${FPS}
