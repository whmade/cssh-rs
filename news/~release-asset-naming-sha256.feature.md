Release archives now follow the standard Rust CLI naming convention
`cssh-rs-v<version>-<target>.zip` (previously `cssh-rs.<version>.zip`)
and ship a `.sha256` checksum sibling you can verify with
`sha256sum -c <archive>.sha256`.
