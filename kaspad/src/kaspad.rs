use crate::imports::*;
use krc721_core::runtime::{Runtime, Service, ServiceError, ServiceResult};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

/// Termination method with which to terminate the kaspad process.
/// This should remain Sigkill until Kaspad learns to terminate
/// rapidly during it's sync process.
#[derive(Default, Debug, Clone, Eq, PartialEq)]
enum TerminationMethod {
    #[default]
    Sigkill,
    Sigterm,
}

struct Inner {
    config: Config,
    path: Option<PathBuf>,
    is_running: Arc<AtomicBool>,
    pid: Mutex<Option<u32>>,
    events: Option<Channel<Events>>,
    task_ctl: DuplexChannel,
    termination_method: TerminationMethod,
}

#[derive(Clone)]
pub struct Kaspad {
    inner: Arc<Inner>,
}

impl Kaspad {
    pub fn new(
        config: Config,
        path: Option<PathBuf>,
        service_events: Option<Channel<Events>>,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                config,
                path,
                is_running: Arc::new(AtomicBool::new(false)),
                pid: Mutex::new(None),
                events: service_events.clone(),
                task_ctl: DuplexChannel::oneshot(),
                termination_method: TerminationMethod::default(),
            }),
        }
    }

    fn inner(&self) -> &Inner {
        &self.inner
    }

    pub fn is_running(&self) -> bool {
        self.inner().is_running.load(Ordering::SeqCst)
    }

    #[cfg(unix)]
    fn sigterm(&self, pid: u32) {
        use nix::sys::signal::Signal;
        use nix::unistd::Pid;
        if let Err(err) = nix::sys::signal::kill(Pid::from_raw(pid as i32), Signal::SIGTERM) {
            println!("kaspad sigterm error: {:?}", err);
        }
    }
}

#[async_trait]
impl Service for Kaspad {
    async fn spawn(self: Arc<Self>, _runtime: Runtime) -> ServiceResult<()> {
        let mut cmd = if let Some(path) = self.inner().path.clone() {
            Command::new(path)
        } else {
            let path = std::env::current_exe()?;
            Command::new(path)
        };

        let cmd = cmd
            .args(self.inner().config.clone())
            .env("KASPA_DAEMON", "1")
            .stdout(Stdio::piped());

        let is_running = self.inner().is_running.clone();
        is_running.store(true, Ordering::SeqCst);
        let mut child = cmd
            .spawn()
            .map_err(Error::NodeStartupError)
            .map_err(ServiceError::custom)?;
        let stdout = child
            .stdout
            .take()
            .ok_or(Error::NodeStdoutHandleError)
            .map_err(ServiceError::custom)?;
        *self.inner.pid.lock().unwrap() = child.id();

        let mut reader = BufReader::new(stdout).lines();
        let stdout_relay_sender = self
            .inner
            .events
            .as_ref()
            .map(|events| events.sender.clone());
        let task_ctl = self.inner.task_ctl.clone();

        let this = self.clone();

        cfg_if::cfg_if! {
            if #[cfg(unix)] {
                let is_unix = true;
            } else {
                let is_unix = false;
            }
        }

        tokio::spawn(async move {
            loop {
                select! {
                    _ = task_ctl.request.recv().fuse() => {
                        if this.inner.termination_method == TerminationMethod::Sigterm && is_unix {
                            let pid = this.inner.pid.lock().unwrap();
                            if let Some(_pid) = *pid {
                                #[cfg(unix)]
                                this.sigterm(_pid);
                            }
                        } else if let Err(err) = child.start_kill() {
                            println!("kaspa daemon start_kill error: {:?}", err);
                        }
                    }
                    status = child.wait().fuse() => {
                        match status {
                            Ok(_status) => {
                                // println!("kaspad shutdown: {:?}", _status);
                            }
                            Err(err) => {
                                println!("kaspad shutdown error: {:?}", err);
                            }
                        }
                        is_running.store(false,Ordering::SeqCst);
                        break;
                    }

                    line = reader.next_line().fuse() => {
                        if let Ok(Some(line)) = line {
                            // println!("kaspad: {}", line);
                            if let Some(sender) = &stdout_relay_sender {
                                sender.send(Events::Stdout { line }).await.unwrap();
                            } else {
                                println!("kaspad: {}", line);
                            }
                        }
                    }
                }
            }

            task_ctl.response.send(()).await.unwrap();
        });

        Ok(())
    }

    fn terminate(self: Arc<Self>) {
        // log_trace!("sending an exit signal to {}", SERVICE);
        self.inner.task_ctl.request.try_send(()).unwrap();
    }

    async fn join(self: Arc<Self>) -> ServiceResult<()> {
        self.inner.task_ctl.response.recv().await?;
        Ok(())
    }
}
