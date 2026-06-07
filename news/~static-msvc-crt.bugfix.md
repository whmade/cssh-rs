Statically link the MSVC C runtime so `cssh-rs.exe` runs on Windows hosts
that do not have the Visual C++ Redistributable installed. Previously
launching the binary on a fresh Windows install failed with
`VCRUNTIME140.dll was not found`. Release builds (and local builds for
the `x86_64-pc-windows-msvc` target) now use `-C target-feature=+crt-static`,
baking the runtime into the binary and preserving cssh-rs's "portable
single-exe" promise.
