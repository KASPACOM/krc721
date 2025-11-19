use crate::imports::*;
use crate::phase::Phase;
use crate::status::Status;
use cliclack::*;
use futures::StreamExt;
use indicatif::ProgressBar;
use krc721_core::utils::separate_bytes;
use reqwest::header::{HeaderMap, HeaderValue};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

const SNAPSHOT_TIMEOUT_HOURS: u64 = 12;

pub struct Receiver {
    pub servers: HashMap<Network, Vec<String>>,
    pub network: Network,
    pub folder: PathBuf,
}

impl Receiver {
    pub fn new(network: Network, folder: PathBuf, servers: HashMap<Network, Vec<String>>) -> Self {
        Self {
            servers,
            network,
            folder,
        }
    }

    fn headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            "SNAPSHOT_API_KEY",
            HeaderValue::from_static("bcdbc3956ffccfef6cca6ea2860cabc4"),
        );
        headers
    }

    async fn request_generate(&self, server: &str) -> Result<Phase> {
        let url = format!("{}/sync/{}/generate", server, self.network);
        let response = reqwest::Client::new()
            .get(&url)
            .headers(Self::headers())
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(Error::custom(format!(
                "Generate request failed: {}",
                response.status()
            )));
        }

        let phase = response.json::<Phase>().await?;
        Ok(phase)
    }

    async fn request_status(&self, server: &str) -> Result<Status> {
        let url = format!("{}/sync/{}/status", server, self.network);
        let response = reqwest::Client::new()
            .get(&url)
            .headers(Self::headers())
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(Error::custom(format!(
                "Status request failed: {}",
                response.status()
            )));
        }

        let status = response.json::<Status>().await?;
        Ok(status)
    }

    async fn request_phase(&self, server: &str) -> Result<Phase> {
        let url = format!("{}/sync/{}/status", server, self.network);
        let response = reqwest::Client::new()
            .get(&url)
            .headers(Self::headers())
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(Error::custom(format!(
                "Status request failed: {}",
                response.status()
            )));
        }

        let status = response.json::<Status>().await?;
        Ok(status.phase)
    }

    pub async fn request_reset(&self, server: &str) -> Result<()> {
        let url = format!("{}/sync/{}/reset", server, self.network);
        let response = reqwest::Client::new()
            .get(&url)
            .headers(Self::headers())
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(Error::custom(format!(
                "Reset request failed: {}",
                response.status()
            )));
        }

        Ok(())
    }

    async fn download(&self, server: &str, filename: &str) -> Result<PathBuf> {
        let url = format!("{}/sync/{}/download/{}", server, self.network, filename);
        let response = reqwest::Client::new()
            .get(&url)
            .headers(Self::headers())
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(Error::custom(format!(
                "Download request failed: {}",
                response.status()
            )));
        }

        let total_size = response
            .content_length()
            .ok_or_else(|| Error::custom("Content-Length header missing"))?;

        let pb = progress_bar(total_size);
        pb.start("Downloading snapshot...");

        let target_path = self.folder.join(filename);
        let mut file = File::create(&target_path).await?;
        let mut stream = response.bytes_stream();

        let mut downloaded = 0;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;
            pb.inc(chunk.len() as u64);
        }

        pb.stop(format!(
            "Snapshot downloaded: {} bytes",
            separate_bytes(downloaded)
        ));
        Ok(target_path)
    }

    pub async fn ensure_available(&self, server: &str) -> Result<Phase> {
        let (current_daa_score, phase) = match self.request_status(server).await {
            Ok(status) => {
                if !status.sync {
                    return Err(Error::custom(format!(
                        "Server is not synced - DAA score: {}",
                        status.daa_score
                    )));
                }
                (status.daa_score, status.phase)
            }
            Err(e) => {
                return Err(e);
            }
        };

        let phase = match &phase {
            Phase::Ready { archive, daa_score } => {
                let daa_score_timeout = self.network.daa_score_per_hour() * SNAPSHOT_TIMEOUT_HOURS;

                if current_daa_score.saturating_sub(daa_score_timeout) > *daa_score {
                    log::info(format!(
                        "Server has outdated snapshot: {} (DAA score: {})",
                        archive, daa_score
                    ))?;

                    log::info("Resetting snapshot")?;
                    self.request_reset(server).await?;
                    match self.request_phase(server).await {
                        Ok(Phase::None) => {
                            log::info("Snapshot reset successful")?;
                            Phase::None
                        }
                        Ok(phase) => {
                            return Err(Error::custom(format!("Unexpected phase: {:?}", phase)));
                        }
                        Err(e) => {
                            return Err(e);
                        }
                    }
                } else {
                    phase
                }
            }
            phase => phase.clone(),
        };

        Ok(phase)
    }

    pub async fn fetch(&self) -> Result<PathBuf> {
        let servers = self.servers.get(&self.network).ok_or_else(|| {
            Error::custom(format!(
                "No servers configured for network {}",
                self.network
            ))
        })?;

        // Try each server until we find one that works
        for server in servers.iter() {
            log::info(format!("Trying server {}", server))?;

            let phase = match self.ensure_available(server).await {
                Ok(phase) => phase,
                Err(e) => {
                    log::error(e.to_string())?;
                    continue;
                }
            };

            // Check server status
            match phase {
                Phase::Ready { archive, daa_score } => {
                    log::info(format!(
                        "Server has ready snapshot: {} (DAA score: {})",
                        archive, daa_score
                    ))?;
                    return self.download(server, &archive).await;
                }
                Phase::None => {
                    log::info("Starting snapshot generation")?;
                    match self.request_generate(server).await {
                        Ok(_) => {
                            let pb = ProgressBar::new(100).with_message("Processing...");

                            // Poll status until ready
                            loop {
                                tokio::time::sleep(std::time::Duration::from_secs(1)).await;

                                let status = match self.request_status(server).await {
                                    Ok(status) => status,
                                    Err(e) => {
                                        pb.abandon();
                                        log::error(format!("Failed to check server status: {e}"))?;
                                        break;
                                    }
                                };

                                match status.phase {
                                    Phase::Archiving { progress } => {
                                        pb.set_position((progress * 100.0) as u64);
                                    }
                                    Phase::Ready { archive, daa_score } => {
                                        pb.finish();
                                        log::info(format!(
                                            "Snapshot ready: {} (DAA score: {})",
                                            archive, daa_score
                                        ))?;
                                        return self.download(server, &archive).await;
                                    }
                                    phase => {
                                        pb.abandon();
                                        log::error(format!("Unexpected phase: {:?}", phase))?;
                                        break;
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            log::error(format!("Failed to initiate generation: {e}"))?;
                        }
                    }
                }
                phase => {
                    log::info(format!("Server busy: {:?}", phase))?;
                }
            }
        }

        Err(Error::custom("No servers available"))
    }
}
