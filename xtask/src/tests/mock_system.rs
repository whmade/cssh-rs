//! One mock for every cross-build and test VM system trait, plus fakes
//! that give it state.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use mockall::mock;

use crate::cross_build::system::windows_msvc::WindowsMsvcSystem;
use crate::cross_build::system::CrossBuildSystem;
use crate::vm::{SshEndpoint, VmSystem};

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
    impl VmSystem for System {
        fn docker_daemon_reachable(&self) -> bool;
        fn path_exists(&self, path: &Path) -> bool;
        fn env_var(&self, key: &str) -> Option<String>;
        fn list_containers(&self) -> anyhow::Result<String>;
        fn volume_exists(&self, name: &str) -> bool;
        fn run_docker(&self, args: &[String]) -> anyhow::Result<()>;
        fn docker_port(&self, container: &str, port: &str) -> anyhow::Result<String>;
        fn create_dir_all(&self, path: &Path) -> anyhow::Result<()>;
        fn read_file(&self, path: &Path) -> anyhow::Result<String>;
        fn write_file(&self, path: &Path, contents: &str) -> anyhow::Result<()>;
        fn generate_ssh_key(&self, path: &Path) -> anyhow::Result<()>;
        fn ssh_ready(&self, endpoint: &SshEndpoint) -> bool;
        fn sleep(&self, duration: Duration);
        fn now(&self) -> Instant;
    }
}

/// The test VM's data directory under the fake `$HOME` of `/home/dev`.
pub const DATA_DIR: &str = "/home/dev/.local/share/cssh-rs/windows-vm";
/// The private SSH key the xtask keeps in [`DATA_DIR`].
pub const KEY: &str = "/home/dev/.local/share/cssh-rs/windows-vm/id_ed25519";
/// The public half of [`KEY`].
pub const PUBLIC_KEY: &str = "/home/dev/.local/share/cssh-rs/windows-vm/id_ed25519.pub";

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

/// A stand-in Docker daemon holding the VM container's state.
///
/// `run` starts the container, `stop` exits it and `rm` removes it, so a
/// test can watch the lifecycle through [`FakeDocker::subcommands`].
#[derive(Clone, Default)]
pub struct FakeDocker {
    state: Arc<Mutex<Option<&'static str>>>,
    calls: Arc<Mutex<Vec<Vec<String>>>>,
}

impl FakeDocker {
    /// Create a daemon whose VM container is in a given state.
    ///
    /// # Arguments
    /// * `state` - `docker ps` state such as `running` or `exited`, or
    ///   `None` for no container.
    ///
    /// # Returns
    /// The fake, to [`FakeDocker::install`] into a mock.
    pub fn new(state: Option<&'static str>) -> Self {
        let docker = Self::default();
        *docker.state.lock().unwrap() = state;
        docker
    }

    /// Route the mock's Docker calls to this fake.
    ///
    /// # Arguments
    /// * `mock` - Mock to answer `docker ps`, `docker port` and `docker`
    ///   calls through this fake.
    pub fn install(&self, mock: &mut MockSystem) {
        let docker = self.clone();
        mock.expect_list_containers().returning(move || {
            Ok(docker
                .state
                .lock()
                .unwrap()
                .map_or_else(String::new, |state| {
                    format!("other\trunning\ncssh-rs-windows-vm\t{state}\n")
                }))
        });
        let docker = self.clone();
        mock.expect_run_docker().returning(move |args| {
            let next = match args[0].as_str() {
                "run" => Some("running"),
                "stop" => Some("exited"),
                _ => None,
            };
            *docker.state.lock().unwrap() = next;
            docker.calls.lock().unwrap().push(args.to_vec());
            Ok(())
        });
        mock.expect_docker_port().returning(|_, port| {
            Ok(match port {
                "22/tcp" => "127.0.0.1:2222\n",
                _ => "127.0.0.1:8006\n",
            }
            .to_owned())
        });
    }

    /// Set the container's state, as if it changed outside the xtask.
    ///
    /// # Arguments
    /// * `state` - New `docker ps` state, or `None` for no container.
    pub fn set_state(&self, state: Option<&'static str>) {
        *self.state.lock().unwrap() = state;
    }

    /// Return the arguments of every `docker` call so far.
    ///
    /// # Returns
    /// One argument list per call, oldest first.
    pub fn calls(&self) -> Vec<Vec<String>> {
        self.calls.lock().unwrap().clone()
    }

    /// Return the subcommand of every `docker` call so far.
    ///
    /// # Returns
    /// One subcommand, such as `run` or `rm`, per call, oldest first.
    pub fn subcommands(&self) -> Vec<String> {
        self.calls()
            .into_iter()
            .map(|args| args[0].clone())
            .collect()
    }
}

/// A stand-in file system holding the files the xtask writes.
#[derive(Clone, Default)]
pub struct FakeFiles {
    files: Arc<Mutex<HashMap<PathBuf, String>>>,
}

impl FakeFiles {
    /// Create a file system holding some files and `/dev/kvm`.
    ///
    /// # Arguments
    /// * `files` - `(path, contents)` pairs that exist from the start.
    ///
    /// # Returns
    /// The fake, to [`FakeFiles::install`] into a mock.
    pub fn new(files: &[(&str, &str)]) -> Self {
        let fake = Self::default();
        for (path, contents) in files.iter().chain(&[("/dev/kvm", "")]) {
            fake.files
                .lock()
                .unwrap()
                .insert(PathBuf::from(path), (*contents).to_owned());
        }
        fake
    }

    /// Route the mock's file calls to this fake.
    ///
    /// # Arguments
    /// * `mock` - Mock to answer file, directory and key calls through
    ///   this fake.
    pub fn install(&self, mock: &mut MockSystem) {
        let fake = self.clone();
        mock.expect_path_exists()
            .returning(move |path| fake.files.lock().unwrap().contains_key(path));
        let fake = self.clone();
        mock.expect_read_file().returning(move |path| {
            fake.files
                .lock()
                .unwrap()
                .get(path)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("{} does not exist", path.display()))
        });
        let fake = self.clone();
        mock.expect_write_file().returning(move |path, contents| {
            fake.files
                .lock()
                .unwrap()
                .insert(path.to_path_buf(), contents.to_owned());
            Ok(())
        });
        let fake = self.clone();
        mock.expect_generate_ssh_key().returning(move |path| {
            let mut files = fake.files.lock().unwrap();
            files.insert(path.to_path_buf(), "PRIVATE".to_owned());
            files.insert(path.with_extension("pub"), "ssh-ed25519 KEY".to_owned());
            Ok(())
        });
        mock.expect_create_dir_all().returning(|_| Ok(()));
    }

    /// Return the contents of a file.
    ///
    /// # Arguments
    /// * `path` - File to read.
    ///
    /// # Returns
    /// The contents, or `None` if the file does not exist.
    pub fn get(&self, path: &str) -> Option<String> {
        self.files.lock().unwrap().get(Path::new(path)).cloned()
    }
}
