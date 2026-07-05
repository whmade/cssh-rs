*** Settings ***
Documentation       First Windows E2E cases for cssh-rs: cluster launch, daemon-focused
...                 fan-out, client-focused gating and teardown.
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
...                 and their windows) is expensive, so the cluster is launched once in
...                 Suite Setup and each behaviour is one atomic test case run in order.
...                 This keeps a failure pinned to a single requirement without paying to
...                 relaunch the cluster per behaviour. Teardown is the Suite Teardown,
...                 the natural home for shutting down a suite-scoped fixture.

Resource            ../resources/cssh_rs_cluster.resource

Suite Setup         Start Cssh Cluster
Suite Teardown      Tear Down Cssh Cluster


*** Variables ***
${CSSH_RS_BINARY}       %{CSSH_RS_BINARY}
@{ALIASES}              alpha    bravo
${CLUSTER_NAME}         e2e


*** Test Cases ***
Cluster Launch Brings Up Daemon And All Client Windows
    [Documentation]    The daemon window and one client window per host come up. Each
    ...                unique focus proves that exactly one window with that title exists.
    Wait Until Keyword Succeeds    15x    1s    Focus Window    ${DAEMON_TITLE}
    FOR    ${alias}    IN    @{ALIASES}
        Wait Until Keyword Succeeds    15x    1s    Focus Window    @${alias}    substring
    END

Daemon-Focused Input Fans Out To Every Host
    [Documentation]    Input typed while the daemon is focused reaches every host's marker.
    ...                Retried until the ssh sessions are connected and accept input.
    ${suffix}=    Generate Random String    16    [LOWER][NUMBERS]
    ${marker}=    Set Variable    FANOUT${suffix}
    Wait Until Keyword Succeeds    30x    1s    Fan Out Line Reaches All Hosts    ${marker}

Client-Focused Input Reaches Only The Focused Host
    [Documentation]    Input typed while a single client is focused reaches only that
    ...                host. The other hosts are checked only after the target confirms
    ...                delivery, which is sound because a focused client console cannot
    ...                route input to another host's ssh session.
    ${suffix}=    Generate Random String    16    [LOWER][NUMBERS]
    ${marker}=    Set Variable    GATED${suffix}
    ${target}=    Set Variable    ${ALIASES}[0]
    Wait Until Keyword Succeeds    30x    1s    Client Line Reaches Only    ${target}    ${marker}
    FOR    ${alias}    IN    @{ALIASES}
        IF    '${alias}' != '${target}'
            ${content}=    Read Marker    ${alias}
            Should Not Contain    ${content}    ${marker}
        END
    END
