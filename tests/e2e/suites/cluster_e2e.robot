*** Settings ***
Documentation       First Windows E2E cases for cssh-rs: cluster launch, daemon-focused
...                 broadcast, client-focused gating and teardown.
...
...                 Windows-only: it drives the real cssh-rs binary and synthesises
...                 keystrokes through the desktop, so it must run on a Windows host
...                 with an OpenSSH server available (the sshd fixture locates sshd on
...                 PATH, at the default OpenSSH install paths, or via CSSH_E2E_SSHD).
...
...                 Pass the binary under test with
...                 --variable CSSH_RS_BINARY:<path-to-cssh-rs.exe> (or the
...                 CSSH_RS_BINARY environment variable).
...
...                 Partitioning: launching a real cluster (sshd plus two ssh sessions
...                 and their windows) is expensive, so it is launched once in Suite
...                 Setup and each behaviour is one atomic case run in order, pinning a
...                 failure to a single requirement without relaunching per case.

Resource            ../resources/cssh_rs_cluster.resource

Suite Setup         Start Cssh Cluster
Suite Teardown      Tear Down Cssh Cluster


*** Variables ***
${CSSH_RS_BINARY}       %{CSSH_RS_BINARY}
@{ALIASES}              alpha    bravo
${CLUSTER_NAME}         e2e


*** Test Cases ***
Cluster Launch Brings Up Daemon And Client Windows
    Assert Daemon Window Appears
    Assert Client Window Appears For Each Host

Broadcast Reaches Every Host When Daemon Focused
    Assert All Ssh Connections Established
    ${message}=    Unique Message    BROADCAST
    Focus Daemon And Broadcast    ${message}
    Wait Until Keyword Succeeds    10x    0.5s    Assert Every Host Received    ${message}

Focused Client Receives Input Alone
    Assert All Ssh Connections Established
    ${message}=    Unique Message    GATED
    ${target}=    Set Variable    ${ALIASES}[0]
    Focus Client And Type    ${target}    ${message}
    Wait Until Keyword Succeeds    10x    0.5s    Assert Only Host Received    ${target}    ${message}
