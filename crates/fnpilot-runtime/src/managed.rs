use fnpilot_core::{Result, session::Session};
use std::{
    net::{Ipv4Addr, SocketAddrV4, TcpListener},
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

/// Owns exactly the child it launched. Never adopts or stops a user's server.
pub struct ManagedRuntime {
    child: Child,
    port: u16,
    #[cfg(windows)]
    job: Option<OwnedJob>,
}
impl ManagedRuntime {
    pub fn launch(
        executable: &Path,
        model: &Path,
        port: u16,
        session: &Session,
        token: u64,
    ) -> Result<Self> {
        session.check(token)?;
        if port == 0 || !executable.is_file() || !model.is_file() {
            return Err("Select an installed helper, GGUF file and nonzero local port.".into());
        }
        let reservation = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port))
            .map_err(|_| "Runtime port is already occupied.")?;
        // llama-server must bind this port itself. Any racing collision remains a startup error;
        // readiness also checks that our actual child is still alive.
        drop(reservation);
        let mut command = Command::new(executable);
        command
            .arg("--model")
            .arg(model)
            .args([
                "--host",
                "127.0.0.1",
                "--port",
                &port.to_string(),
                "--parallel",
                "1",
                "--alias",
                "local-gguf",
                "--ctx-size",
                "4096",
                "--threads",
                "2",
                "--threads-batch",
                "2",
                "--jinja",
                "--no-webui",
                "--log-disable",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        command.args([
            "--n-gpu-layers",
            if cfg!(target_os = "macos") { "99" } else { "0" },
        ]);
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let child = command.spawn().map_err(
            |_| "The local helper could not start. Check its dependent native libraries.",
        )?;
        let mut owned = Self {
            child,
            port,
            #[cfg(windows)]
            job: None,
        };
        #[cfg(windows)]
        {
            owned.job = Some(OwnedJob::attach(&owned.child)?);
        }
        let deadline = Instant::now() + Duration::from_secs(30);
        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_millis(500))
            .build()
            .map_err(|_| "Runtime health client failed.")?;
        while Instant::now() < deadline {
            session.check(token)?;
            if owned
                .child
                .try_wait()
                .map_err(|_| "Could not inspect helper.")?
                .is_some()
            {
                return Err("The owned runtime exited before readiness.".into());
            }
            if let Ok(response) = client.get(format!("http://127.0.0.1:{port}/health")).send()
                && response.status() == 200
            {
                if listener_owned_by(port, owned.child.id())?
                    && owned
                        .child
                        .try_wait()
                        .map_err(|_| "Could not inspect helper.")?
                        .is_none()
                {
                    session.check(token)?;
                    return Ok(owned);
                }
                return Err("The runtime listener does not belong to the launched helper.".into());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        Err("Local runtime startup timed out.".into())
    }
    pub fn port(&self) -> u16 {
        self.port
    }
    pub fn running(&mut self) -> Result<bool> {
        self.child
            .try_wait()
            .map(|status| status.is_none())
            .map_err(|_| "Could not inspect owned helper.".into())
    }
}

#[cfg(windows)]
struct OwnedJob(usize);
#[cfg(windows)]
impl OwnedJob {
    fn attach(child: &Child) -> Result<Self> {
        use std::os::windows::io::AsRawHandle;
        use windows::Win32::{Foundation::HANDLE, System::JobObjects::*};
        unsafe {
            let handle = CreateJobObjectW(None, None)
                .map_err(|_| "Could not create an owned runtime job.")?;
            let job = Self(handle.0 as usize);
            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of_val(&limits) as u32,
            )
            .map_err(|_| "Could not configure runtime process-tree cleanup.")?;
            AssignProcessToJobObject(handle, HANDLE(child.as_raw_handle()))
                .map_err(|_| "Could not assign the owned helper to its process job.")?;
            Ok(job)
        }
    }
}
#[cfg(windows)]
impl Drop for OwnedJob {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(windows::Win32::Foundation::HANDLE(
                self.0 as *mut _,
            ));
        }
    }
}
fn listener_owned_by(port: u16, pid: u32) -> Result<bool> {
    #[cfg(target_os = "macos")]
    {
        let output = Command::new("/usr/sbin/lsof")
            .args(["-nP", &format!("-iTCP:{port}"), "-sTCP:LISTEN", "-t"])
            .output()
            .map_err(|_| "Cannot verify runtime listener ownership.")?;
        let ids = String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        Ok(!ids.is_empty() && ids.iter().all(|id| id.parse::<u32>() == Ok(pid)))
    }
    #[cfg(target_os = "windows")]
    {
        let output = Command::new("netstat.exe")
            .args(["-ano", "-p", "tcp"])
            .output()
            .map_err(|_| "Cannot verify runtime listener ownership.")?;
        let text = String::from_utf8_lossy(&output.stdout);
        Ok(text
            .lines()
            .filter_map(|line| {
                let cols = line.split_whitespace().collect::<Vec<_>>();
                (cols.len() == 5
                    && cols[0] == "TCP"
                    && cols[1] == format!("127.0.0.1:{port}")
                    && cols[3] == "LISTENING")
                    .then(|| cols[4].parse::<u32>().ok())
                    .flatten()
            })
            .any(|owner| owner == pid))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (port, pid);
        Err("Managed runtime listener verification is supported on macOS and Windows.".into())
    }
}
impl Drop for ManagedRuntime {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            // A reaped PID/group can be reused. Only signal a group while its
            // original leader is still our unreaped child.
            if matches!(self.child.try_wait(), Ok(None)) {
                unsafe {
                    libc::kill(-(self.child.id() as i32), libc::SIGTERM);
                    libc::kill(-(self.child.id() as i32), libc::SIGKILL);
                }
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
