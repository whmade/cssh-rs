*** Settings ***
Documentation       Keywords for suites whose hosts read raw virtual terminal input, so ESC
...                 and control bytes reach the markers instead of being consumed by line
...                 editing.

Resource            cssh_rs_cluster.resource


*** Keywords ***
Start Raw Input Cluster
    [Documentation]    Start the cluster with raw-input markers and wait until every window
    ...                and ssh session is up.
    ${raw_markers}=    Evaluate    cssh_rs_automation.sshd_fixture.MarkerMode(raw_input=True)
    ...    modules=cssh_rs_automation.sshd_fixture
    Start Cssh Cluster    sshd_mode=${raw_markers}
    Assert Daemon Window Appears
    Assert Client Window Appears For Each Host
    Assert All Ssh Connections Established

Assert Every Host Received Bytes
    [Documentation]    Assert every host's marker contains ${bytes}, a substring check
    ...                because Split To Lines breaks lines at 0x1c and 0x1d.
    [Arguments]    ${bytes}
    FOR    ${alias}    IN    @{ALIASES}
        ${content}=    Read Marker    ${alias}
        Should Contain    ${content}    ${bytes}
        ...    msg=Marker for ${alias} lacks ${{ascii($bytes)}}: ${{ascii($content)}}
        ...    values=False
    END

Keys Typed In Daemon Reach Every Host As
    [Documentation]    Press ${keys} by scan code in the daemon between two unique texts,
    ...                then assert every host received ${bytes} exactly between them.
    [Arguments]    ${bytes}    @{keys}
    ${prefix}=    Unique Message    BEFORE
    ${suffix}=    Unique Message    AFTER
    Focus Window    ${DAEMON_TITLE}
    Type Text    ${prefix}
    Press Keys With Scan Codes    @{keys}
    Type Text    ${suffix}
    Press Key    enter
    Wait Until Keyword Succeeds    10x    0.5s
    ...    Assert Every Host Received Bytes    ${prefix}${bytes}${suffix}
