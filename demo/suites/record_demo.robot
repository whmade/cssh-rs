*** Settings ***
Documentation       Record the cssh-rs demo GIF - a tour of the core features.
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
Record The Feature Tour
    Start Demo    ${BINARY}    ${OUTPUT_DIR}    fps=${FPS}
    Wait For Hosts

    # Add the fourth host live, then broadcast into every client's demo/data.
    Add Host    hostb.dev
    Hold    2
    Broadcast    cd demo/data
    Broadcast    ll
    Hold    2

    # Only hosta.dev ships the README; read and copy it from that one client.
    Focus Client    hosta.dev
    Type Command    cat README
    Copy Readme
    Hold    2

    # Silence hosta.dev (bottom-left), then write the README into the rest.
    Disable Client    down
    Edit Readme
    Hold    2

    # Re-enable everyone; the README now reads the same on every host.
    Enable All
    Broadcast    cat README
    Hold    2

    # A broadcast ping shows the whole cluster reacting; Ctrl+C aborts it.
    Broadcast    ping google.com
    Hold    1
    Interrupt
    Hold    2

    Export Demo Gif    ${GIF}    fps=${FPS}
