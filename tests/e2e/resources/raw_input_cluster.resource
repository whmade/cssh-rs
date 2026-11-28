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
    [Documentation]    Assert every host's marker contains ${bytes}.
    [Arguments]    ${bytes}
    Assert Hosts Received Bytes    ${ALIASES}    ${bytes}

Assert Hosts Received Bytes
    [Documentation]    Assert the marker of every alias in ${aliases} contains ${bytes}, a
    ...                substring check because Split To Lines breaks lines at 0x1c and 0x1d.
    [Arguments]    ${aliases}    ${bytes}
    FOR    ${alias}    IN    @{aliases}
        ${content}=    Read Marker    ${alias}
        Should Contain    ${content}    ${bytes}
        ...    msg=Marker for ${alias} lacks ${{ascii($bytes)}}: ${{ascii($content)}}
        ...    values=False
    END

Keys Typed In Daemon Reach Every Host As
    [Documentation]    Press ${keys} by scan code in the daemon between two unique texts,
    ...                then assert every host received ${bytes} exactly between them.
    [Arguments]    ${bytes}    @{keys}
    Focus Window    ${DAEMON_TITLE}
    ${prefix}    ${suffix}=    Type Keys Between Unique Texts    @{keys}
    Wait Until Keyword Succeeds    10x    0.5s
    ...    Assert Every Host Received Bytes    ${prefix}${bytes}${suffix}

Keys Typed At A Client Reach Its Host As
    [Documentation]    Press ${keys} by scan code in the ${target} client window between two
    ...                unique texts, then assert ${target} received ${bytes} exactly between
    ...                them.
    [Arguments]    ${target}    ${bytes}    @{keys}
    Focus Window    @${target}    substring
    ${prefix}    ${suffix}=    Type Keys Between Unique Texts    @{keys}
    @{hosts}=    Create List    ${target}
    Wait Until Keyword Succeeds    10x    0.5s
    ...    Assert Hosts Received Bytes    ${hosts}    ${prefix}${bytes}${suffix}

Type Keys Between Unique Texts
    [Documentation]    In the focused window, type a unique text, press ${keys} by scan code,
    ...                type a second unique text and press Enter. Return the two texts.
    [Arguments]    @{keys}
    ${prefix}=    Unique Message    BEFORE
    ${suffix}=    Unique Message    AFTER
    Type Text    ${prefix}
    Press Keys With Scan Codes    @{keys}
    Type Text    ${suffix}
    Press Key    enter
    RETURN    ${prefix}    ${suffix}
