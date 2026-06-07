## Changelog

{{CHANGELOG}}

## Pre-requisite
- Any SSH client (Windows 10 and Windows 11 already include a built-in SSH server and client - [docs](https://learn.microsoft.com/en-us/windows/terminal/tutorials/ssh))
- Microsoft Visual C++ Redistributable (already present on most Windows systems; required if launching `cssh-rs.exe` fails with a `VCRUNTIME140.dll was not found` error - [download](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist))

## Download/Installation
cssh-rs is a portable application and is not installed.
Just download the `.zip` archive, extract it and run `cssh-rs.exe`

> [!NOTE]
> `cssh-rs.exe` is not code-signed. On first launch Windows SmartScreen
> shows a "Windows protected your PC" warning because the binary was
> downloaded from the internet and has no recognized publisher. To run
> it: click `More info` -> `Run anyway`, or right-click `cssh-rs.exe` ->
> `Properties` -> tick `Unblock` -> `Apply`. Equivalent from
> PowerShell: `Unblock-File .\cssh-rs.exe`.

# <a href="https://github.com/whmade/cssh-rs/releases/download/{{VERSION}}/cssh-rs.{{VERSION}}.zip"><img src="https://raw.githubusercontent.com/whmade/cssh-rs/refs/heads/main/cssh-rs/res/cssh-rs.svg" width="20" alt="cssh-rs Logo" /> cssh-rs.{{VERSION}}.zip</a> [![Downloads](https://img.shields.io/github/downloads/whmade/cssh-rs/{{VERSION}}/total?label=downloads)](https://github.com/whmade/cssh-rs/releases/download/{{VERSION}}/cssh-rs.{{VERSION}}.zip)

### Verifying the download
Starting with 0.19.0 the release `.zip` is signed with a [GitHub build
attestation](https://docs.github.com/en/actions/security-for-github-actions/using-artifact-attestations/using-artifact-attestations-to-establish-provenance-for-builds)
produced by this repository's release workflow. Verifying the
attestation cryptographically proves that the archive came from the
`whmade/cssh-rs` release workflow at the tagged commit - i.e. the `.zip`
you have is byte-for-byte the one GitHub built for this release and
was not modified or repackaged after upload.

Verify it with the [GitHub CLI](https://cli.github.com/):
```sh
gh attestation verify cssh-rs.{{VERSION}}.zip --repo whmade/cssh-rs
```
Note that this verifies the `.zip` only; the `cssh-rs.exe` inside it is
still unsigned, so the SmartScreen warning above still applies.
