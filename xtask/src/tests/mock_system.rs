//! One mock for every cross-build system trait.

use mockall::mock;

use crate::cross_build::system::windows_msvc::WindowsMsvcSystem;
use crate::cross_build::system::CrossBuildSystem;

mock! {
    pub System {}
    impl CrossBuildSystem for System {
        fn host_os(&self) -> &'static str;
        fn host_arch(&self) -> &'static str;
        fn host_is_msvc(&self) -> bool;
        fn list_installed_targets(&self) -> anyhow::Result<String>;
        fn install_target(&self, triple: &str) -> anyhow::Result<()>;
        fn is_executable_in_path(&self, name: &str) -> bool;
        fn list_cargo_subcommands(&self) -> anyhow::Result<String>;
        fn install_cargo_subcommand(&self, crate_name: &str, version: &str) -> anyhow::Result<()>;
        fn run_cargo(&self, args: &[String], envs: &[(String, String)]) -> anyhow::Result<()>;
    }
    impl WindowsMsvcSystem for System {
        fn read_cargo_xwin_version(&self) -> anyhow::Result<String>;
    }
}

/// Return owned copies of some string slices.
///
/// # Arguments
/// * `items` - Strings to copy.
///
/// # Returns
/// The copies, in order.
pub fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}
