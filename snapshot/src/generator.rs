use crate::imports::*;
use crate::phase::{Phase, PhaseContext};
use crate::progress::Progress;
use crate::snapshot::Snapshot;
use crate::status::Status;
use axum::{
    body::Body,
    extract::Path,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use krc721_core::utils::separate_bytes;
use krc721_nexus::state::State;
use krc721_nexus::syncer::{Syncer, SyncerT};
use serde::{Deserialize, Serialize};
use tokio::fs::File;
use tokio_util::io::ReaderStream;

pub const SNAPSHOT_API_KEY: &str = "bcdbc3956ffccfef6cca6ea2860cabc4";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Filename {
    filename: String,
}

pub struct Generator {
    pub db: Arc<Db>,
    pub network: Network,
    pub folder: PathBuf,
    pub filename: RwLock<Option<PathBuf>>,
    pub phase: RwLock<PhaseContext>,
    pub state: Arc<State>,
    pub syncer: Arc<Syncer>,
    pub check_headers: bool,
}

impl Generator {
    pub fn new(
        db: Arc<Db>,
        network: Network,
        state: Arc<State>,
        syncer: Arc<Syncer>,
        folder: PathBuf,
    ) -> Self {
        Self {
            db,
            network,
            folder,
            filename: RwLock::new(None),
            state,
            syncer,
            phase: RwLock::new(PhaseContext::None),
            check_headers: false,
        }
    }

    pub fn folder(&self) -> PathBuf {
        self.folder.clone()
    }

    pub fn filename(&self) -> Option<PathBuf> {
        self.filename.read().unwrap().clone()
    }

    pub fn set_filename(&self, filename: PathBuf) {
        *self.filename.write().unwrap() = Some(filename);
    }

    pub fn phase(&self) -> Phase {
        self.phase_context().into()
    }

    pub fn phase_context(&self) -> PhaseContext {
        self.phase.read().unwrap().clone()
    }

    pub fn set_phase(&self, phase: PhaseContext) {
        *self.phase.write().unwrap() = phase;
    }

    pub async fn reset(self: &Arc<Self>) -> Result<()> {
        info!("Resetting snapshot folder and phase");
        self.set_phase(PhaseContext::None);
        std::fs::remove_dir_all(&self.folder)?;
        std::fs::create_dir_all(&self.folder)?;
        info!("Snapshot folder and phase reset complete");
        Ok(())
    }

    pub async fn generate(self: &Arc<Self>) -> Result<()> {
        if !matches!(self.phase_context(), PhaseContext::None) {
            // return Err(Error::custom("Snapshot already in progress"));
            return Ok(());
        }

        let progress = Arc::new(Progress::default());
        self.set_phase(PhaseContext::Archiving {
            progress: progress.clone(),
        });

        let this = self.clone();
        tokio::task::spawn_blocking(move || {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async move {
                let daa_score = this.state.current_daa_score();
                let archive = PathBuf::from(format!("snapshot-{daa_score}.krc721"));
                let filename = this.folder.join(&archive);

                info!("Generating snapshot: {}", filename.display());

                let snapshot = Snapshot::default()
                    .with_archive(&filename)
                    .with_progress(progress.clone())
                    .skip_partitions(vec!["notification_queue"]);

                let partition_snapshots = this.db.take_snapshots();
                match snapshot.archive_snapshots(partition_snapshots).await {
                    Ok(header) => {
                        info!("{header}");
                    }
                    Err(err) => {
                        this.set_phase(PhaseContext::None);
                        error!("{err}");
                    }
                }

                this.set_phase(PhaseContext::Ready { archive, daa_score });
                info!("Snapshot is ready: {}", filename.display());
                if let Ok(metadata) = std::fs::metadata(&filename) {
                    let size = separate_bytes(metadata.len());
                    info!("Snapshot size: {size} bytes",);
                } else {
                    error!("Failed to get snapshot metadata");
                }
            });
        });

        Ok(())
    }

    // --------------------------------------------------------------------------

    pub fn register_handlers(self: &Arc<Self>, mut router: Router) -> Router {
        let network = self.network;

        let this = self.clone();
        router = router.route(
            &format!("/sync/{network}/generate"),
            get(|headers: HeaderMap| async move { this.snapshot_generate(headers).await }),
        );

        let this = self.clone();
        router = router.route(
            &format!("/sync/{network}/reset"),
            get(|headers: HeaderMap| async move { this.snapshot_reset(headers).await }),
        );

        let this = self.clone();
        router = router.route(
            &format!("/sync/{network}/status"),
            get(|headers: HeaderMap| async move { this.snapshot_status(headers).await }),
        );

        let this = self.clone();
        router = router.route(
            &format!("/sync/{network}/daa"),
            get(|headers: HeaderMap| async move { this.snapshot_status(headers).await }),
        );

        let this = self.clone();
        router = router.route(
            &format!("/sync/{network}/download/{{filename}}"),
            get(
                |Path(Filename { filename }), headers: HeaderMap| async move {
                    this.snapshot_download(headers, &filename).await
                },
            ),
        );
        router
    }

    fn check_headers(&self, headers: &HeaderMap) -> bool {
        if !self.check_headers {
            return true;
        }

        if let Some(value) = headers.get("SNAPSHOT_API_KEY") {
            if value.to_str().unwrap() == SNAPSHOT_API_KEY {
                return true;
            }
        }

        false
    }

    // --------------------------------------------------------------------------

    async fn snapshot_generate(self: &Arc<Self>, _headers: HeaderMap) -> impl IntoResponse {
        if !self.check_headers(&_headers) {
            return Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .body(Body::from("Unauthorized"))
                .unwrap();
        }

        if !self.syncer.is_synced() {
            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from("Indexer is not synced"))
                .unwrap();
        }

        match self.generate().await {
            Ok(_) => {
                let phase = self.phase();

                Response::builder()
                    .status(StatusCode::OK)
                    .body(Body::from(serde_json::to_string(&phase).unwrap()))
                    .unwrap()
            }
            Err(err) => {
                error!("{err}");
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::from(format!("Error generating snapshot: {err}")))
                    .unwrap()
            }
        }
    }

    async fn snapshot_reset(self: &Arc<Self>, _headers: HeaderMap) -> impl IntoResponse {
        match self.reset().await {
            Ok(_) => Response::builder()
                .status(StatusCode::OK)
                .body(Body::from("Resetting snapshot..."))
                .unwrap(),
            Err(err) => {
                error!("{err}");
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::from(format!("Error resetting snapshot: {err}")))
                    .unwrap()
            }
        }
    }

    async fn snapshot_status(self: &Arc<Self>, _headers: HeaderMap) -> impl IntoResponse {
        let phase = self.phase();
        let sync = self.syncer.is_synced();
        let daa_score = self.state.current_daa_score();

        let status = Status {
            sync,
            phase,
            daa_score,
        };

        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
            .body(Body::from(serde_json::to_string(&status).unwrap()))
            .unwrap()
    }

    async fn snapshot_download(
        self: &Arc<Self>,
        _headers: HeaderMap,
        filename: &str,
    ) -> impl IntoResponse {
        let Phase::Ready {
            archive,
            daa_score: _,
        } = self.phase()
        else {
            return Response::builder()
                .status(StatusCode::METHOD_NOT_ALLOWED)
                .body(Body::from("Snapshot is not ready"))
                .unwrap();
        };

        if archive != filename {
            return Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("Archive not found"))
                .unwrap();
        }

        // Attempt to open the file
        let file = match File::open(self.folder().join(&archive)).await {
            Ok(file) => file,
            Err(err) => {
                error!("Failed to open archive `{archive}`: {err}");
                return Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(Body::from("Archive data file not found"))
                    .unwrap();
            }
        };

        // Get file metadata for content length
        let metadata = match file.metadata().await {
            Ok(metadata) => metadata,
            Err(err) => {
                error!("Failed to get archive metadata: {}", err);
                return Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::from("Failed to get archive metadata"))
                    .unwrap();
            }
        };

        // Convert the file into a stream
        let stream = ReaderStream::new(file);
        let body = Body::from_stream(stream);

        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/octet-stream")
            .header(
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{archive}\""),
            )
            .header(header::CONTENT_LENGTH, metadata.len())
            .body(body)
            .unwrap()
    }
}
