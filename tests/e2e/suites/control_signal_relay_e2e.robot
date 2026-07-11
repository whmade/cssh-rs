*** Settings ***
Documentation       Windows E2E case: a daemon-relayed Ctrl+C interrupts each client's
...                 cooked-mode ssh child instead of landing as a literal ^C byte. With the
...                 daemon focused, cssh-rs broadcasts Ctrl+C to every client; each client's
...                 ssh child runs in the default processed-input console mode, so the relay
...                 must raise a console control signal that ends the session, exactly as if
...                 Ctrl+C had been pressed in that client's own window.
...
...                 The interrupt is proven by every client exiting afterwards: an interrupted
...                 ssh child either lets its client exit outright, or drops the client into the
...                 "SSH connection lost" state that a relayed Shift+Alt+C then closes. A Ctrl+C
...                 that did nothing (issue #144's regression) leaves every window open and this
...                 case times out. A baseline broadcast first confirms the sessions were live
...                 and forwarding right before the interrupt.

Resource            ../resources/cssh_rs_cluster.resource

Suite Setup         Start Cluster And Await Readiness
Suite Teardown      Tear Down Cssh Cluster


*** Variables ***
${CSSH_RS_BINARY}       %{CSSH_RS_BINARY}
@{ALIASES}              alpha    bravo
${CLUSTER_NAME}         e2e


*** Test Cases ***
Relayed Ctrl C Interrupts Every Cooked Mode Ssh Child
    [Documentation]    Confirm every host is receiving broadcasts, relay Ctrl+C from the daemon,
    ...                then assert every client exits because its ssh child was interrupted.
    Broadcast And Assert Every Host Received    BASELINE
    Relay Ctrl C From Daemon
    Assert Every Client Exits After Interrupt


*** Keywords ***
Start Cluster And Await Readiness
    [Documentation]    Start the cluster and wait for the daemon, every client window and every
    ...                ssh session to be ready.
    Start Cssh Cluster
    Assert Daemon Window Appears
    Assert Client Window Appears For Each Host
    Assert All Ssh Connections Established

Broadcast And Assert Every Host Received
    [Documentation]    Broadcast a unique message built from ${prefix} and assert every host
    ...                received it, proving the sessions are live before the interrupt.
    [Arguments]    ${prefix}
    ${message}=    Unique Message    ${prefix}
    Focus Daemon And Broadcast    ${message}
    Wait Until Keyword Succeeds    10x    0.5s    Assert Every Host Received    ${message}

Relay Ctrl C From Daemon
    [Documentation]    Focus the daemon and press Ctrl+C so cssh-rs broadcasts it to every
    ...                client's ssh session, just like any other keystroke.
    Focus Window    ${DAEMON_TITLE}
    Send Hotkey    ctrl    c

Assert Every Client Exits After Interrupt
    [Documentation]    Every client whose ssh child was interrupted exits: a child that exits
    ...                cleanly closes its client outright, and one that exits on the control
    ...                signal drops the client into its "SSH connection lost" state, which a
    ...                relayed Shift+Alt+C closes. The relay is retried per client so it lands
    ...                once the child has actually gone; a Ctrl+C that did nothing leaves the
    ...                window open and this times out.
    FOR    ${alias}    IN    @{ALIASES}
        Wait Until Keyword Succeeds    20x    0.5s    Nudge Then Assert Client Gone    ${alias}
    END

Nudge Then Assert Client Gone
    [Documentation]    Relay a Shift+Alt+C exit nudge (best effort - the daemon window is gone
    ...                once the last client exits), then assert ${alias}'s window is gone.
    [Arguments]    ${alias}
    Run Keyword And Ignore Error    Relay Shift Alt C From Daemon
    Assert Client Window Gone    ${alias}

Relay Shift Alt C From Daemon
    [Documentation]    Focus the daemon and press Shift+Alt+C so cssh-rs broadcasts the client
    ...                exit combination to every client.
    Focus Window    ${DAEMON_TITLE}
    Send Hotkey    shift    alt    c
