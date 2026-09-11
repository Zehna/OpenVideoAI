use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use uuid::Uuid;

use super::compiler::FfmpegCompiler;
use super::renderspec::RenderSpecV1;
use crate::artifacts::importer::ArtifactImporter;
use crate::artifacts::metadata::probe_media;
use crate::artifacts::model::ArtifactRef;
use crate::artifacts::resolver::ArtifactResolver;
use crate::jobs::manager::JobManager;
use crate::jobs::model::{Job, JobState};

pub struct LocalRenderWorkerHandle {
    shutdown_tx: watch::Sender<bool>,
    join_handle: JoinHandle<()>,
}

impl LocalRenderWorkerHandle {
    pub async fn shutdown(self) {
        let _ = self.shutdown_tx.send(true);
        let _ = self.join_handle.await;
    }
}

pub struct LocalRenderWorker {
    pub worker_id: String,
    pub job_manager: Arc<JobManager>,
    pub resolver: Arc<ArtifactResolver>,
    pub importer: Arc<ArtifactImporter>,
    pub staging_dir: PathBuf,
    pub ffmpeg_path: PathBuf,
    pub ffprobe_path: Option<PathBuf>,
    pub is_mock: bool,
}

impl LocalRenderWorker {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        worker_id: String,
        job_manager: Arc<JobManager>,
        resolver: Arc<ArtifactResolver>,
        importer: Arc<ArtifactImporter>,
        staging_dir: PathBuf,
        ffmpeg_path: PathBuf,
        ffprobe_path: Option<PathBuf>,
        is_mock: bool,
    ) -> Self {
        Self {
            worker_id,
            job_manager,
            resolver,
            importer,
            staging_dir,
            ffmpeg_path,
            ffprobe_path,
            is_mock,
        }
    }

    pub fn process_one_job(&self, job: &Job) -> Result<ArtifactRef, String> {
        let current = self
            .job_manager
            .get_job(&job.id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Job not found".to_string())?;
        if current.status == JobState::Cancelled {
            return Err("Job was cancelled".to_string());
        }

        // 1. validate_spec
        self.job_manager
            .record_checkpoint_with_token(
                &job.id,
                job.lease_token.as_deref(),
                "validate_spec",
                "{}",
            )
            .map_err(|e| e.to_string())?;

        let temp_output = self
            .staging_dir
            .join(format!("output_{}.mp4", Uuid::new_v4()));

        if self.is_mock {
            // 2. resolve_artifacts
            self.job_manager
                .record_checkpoint_with_token(
                    &job.id,
                    job.lease_token.as_deref(),
                    "resolve_artifacts",
                    "{}",
                )
                .map_err(|e| e.to_string())?;
            // 3. compile_ffmpeg
            self.job_manager
                .record_checkpoint_with_token(
                    &job.id,
                    job.lease_token.as_deref(),
                    "compile_ffmpeg",
                    "{}",
                )
                .map_err(|e| e.to_string())?;
            // 4. render_media
            self.job_manager
                .record_checkpoint_with_token(
                    &job.id,
                    job.lease_token.as_deref(),
                    "render_media",
                    "{}",
                )
                .map_err(|e| e.to_string())?;

            std::fs::write(&temp_output, b"mock render output mp4").map_err(|e| e.to_string())?;

            // 5. validate_output
            self.job_manager
                .record_checkpoint_with_token(
                    &job.id,
                    job.lease_token.as_deref(),
                    "validate_output",
                    "{}",
                )
                .map_err(|e| e.to_string())?;
            // 6. import_output_artifact
            self.job_manager
                .record_checkpoint_with_token(
                    &job.id,
                    job.lease_token.as_deref(),
                    "import_output_artifact",
                    "{}",
                )
                .map_err(|e| e.to_string())?;

            let project_id = job.project_id.as_deref().unwrap_or("p1");
            let imported = self
                .importer
                .import(project_id, &temp_output)
                .map_err(|e| e.to_string())?;
            let _ = std::fs::remove_file(&temp_output);

            // 7. finalize
            self.job_manager
                .record_checkpoint_with_token(&job.id, job.lease_token.as_deref(), "finalize", "{}")
                .map_err(|e| e.to_string())?;

            let result_json =
                serde_json::to_string(&imported.artifact).map_err(|e| e.to_string())?;
            self.job_manager
                .complete_job_with_token(&job.id, job.lease_token.as_deref(), Some(result_json))
                .map_err(|e| e.to_string())?;

            return Ok(imported.artifact);
        }

        let parsed_payload: serde_json::Value =
            serde_json::from_str(&job.payload).map_err(|e| e.to_string())?;
        let spec: RenderSpecV1 = serde_json::from_value(parsed_payload["renderSpec"].clone())
            .map_err(|e| format!("Invalid RenderSpec: {}", e))?;

        // 2. resolve_artifacts
        self.job_manager
            .record_checkpoint_with_token(
                &job.id,
                job.lease_token.as_deref(),
                "resolve_artifacts",
                "{}",
            )
            .map_err(|e| e.to_string())?;
        let mut resolved_map = HashMap::new();
        for track in &spec.tracks {
            for clip in &track.clips {
                if !resolved_map.contains_key(&clip.artifact_id) {
                    let path = self
                        .resolver
                        .resolve_uri(&format!("artifact://{}", clip.artifact_id))
                        .map_err(|e| format!("Artifact {} not found: {}", clip.artifact_id, e))?;
                    resolved_map.insert(clip.artifact_id.clone(), path);
                }
            }
        }

        // 3. compile_ffmpeg
        self.job_manager
            .record_checkpoint_with_token(
                &job.id,
                job.lease_token.as_deref(),
                "compile_ffmpeg",
                "{}",
            )
            .map_err(|e| e.to_string())?;
        let plan = FfmpegCompiler::compile(&spec, &resolved_map, temp_output.clone())
            .map_err(|e| e.to_string())?;

        // 4. render_media
        self.job_manager
            .record_checkpoint_with_token(&job.id, job.lease_token.as_deref(), "render_media", "{}")
            .map_err(|e| e.to_string())?;

        let mut child = Command::new(&self.ffmpeg_path)
            .args(&plan.args)
            .spawn()
            .map_err(|e| format!("Failed to spawn FFmpeg: {}", e))?;

        let status = loop {
            match child.try_wait() {
                Ok(Some(s)) => break s,
                Ok(None) => {
                    std::thread::sleep(Duration::from_millis(500));
                    match self.job_manager.heartbeat_with_token(
                        &job.id,
                        job.lease_token.as_deref(),
                        60,
                    ) {
                        Ok(true) => {
                            if let Ok(Some(j)) = self.job_manager.get_job(&job.id) {
                                if j.status == JobState::Cancelled {
                                    let _ = child.kill();
                                    let _ = std::fs::remove_file(&temp_output);
                                    return Err("Job was cancelled".to_string());
                                }
                            }
                        }
                        _ => {
                            let _ = child.kill();
                            let _ = std::fs::remove_file(&temp_output);
                            return Err("Lost job lease fence".to_string());
                        }
                    }
                }
                Err(e) => {
                    let _ = child.kill();
                    let _ = std::fs::remove_file(&temp_output);
                    return Err(format!("Error waiting on FFmpeg: {}", e));
                }
            }
        };

        if !status.success() {
            let _ = std::fs::remove_file(&temp_output);
            return Err("FFmpeg exited with error".to_string());
        }

        // 5. validate_output
        self.job_manager
            .record_checkpoint_with_token(
                &job.id,
                job.lease_token.as_deref(),
                "validate_output",
                "{}",
            )
            .map_err(|e| e.to_string())?;
        if !temp_output.is_file()
            || std::fs::metadata(&temp_output)
                .map(|m| m.len())
                .unwrap_or(0)
                == 0
        {
            let _ = std::fs::remove_file(&temp_output);
            return Err("Render output file is empty or missing".to_string());
        }

        let probe = probe_media("render_probe", &temp_output, self.ffprobe_path.as_deref());
        if probe.probe_status == "ok" && probe.duration_ms.unwrap_or(0) == 0 {
            let _ = std::fs::remove_file(&temp_output);
            return Err("Rendered media probe reported 0 duration".to_string());
        }

        // 6. import_output_artifact
        self.job_manager
            .record_checkpoint_with_token(
                &job.id,
                job.lease_token.as_deref(),
                "import_output_artifact",
                "{}",
            )
            .map_err(|e| e.to_string())?;
        let imported = self
            .importer
            .import(&spec.project_id, &temp_output)
            .map_err(|e| e.to_string())?;
        let _ = std::fs::remove_file(&temp_output);

        // 7. finalize
        self.job_manager
            .record_checkpoint_with_token(&job.id, job.lease_token.as_deref(), "finalize", "{}")
            .map_err(|e| e.to_string())?;
        let result_json = serde_json::to_string(&imported.artifact).map_err(|e| e.to_string())?;
        self.job_manager
            .complete_job_with_token(&job.id, job.lease_token.as_deref(), Some(result_json))
            .map_err(|e| e.to_string())?;

        Ok(imported.artifact)
    }
}

#[allow(clippy::too_many_arguments)]
pub fn start_local_render_worker(
    worker_id: String,
    job_manager: Arc<JobManager>,
    resolver: Arc<ArtifactResolver>,
    importer: Arc<ArtifactImporter>,
    staging_dir: PathBuf,
    ffmpeg_path: PathBuf,
    ffprobe_path: Option<PathBuf>,
    poll_interval: Duration,
    is_mock: bool,
) -> LocalRenderWorkerHandle {
    let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
    let worker = Arc::new(LocalRenderWorker::new(
        worker_id,
        job_manager.clone(),
        resolver,
        importer,
        staging_dir,
        ffmpeg_path,
        ffprobe_path,
        is_mock,
    ));

    static RUNTIME: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();

    let rt_handle = match tokio::runtime::Handle::try_current() {
        Ok(h) => h,
        Err(_) => RUNTIME
            .get_or_init(|| {
                tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .expect("failed to initialize background tokio runtime for render worker")
            })
            .handle()
            .clone(),
    };

    let join_handle = rt_handle.spawn(async move {
        loop {
            if *shutdown_rx.borrow() {
                break;
            }

            match job_manager.acquire_next_job_for_type("render.timeline", 60) {
                Ok(Some(job)) => {
                    let w = worker.clone();
                    let _ = tokio::task::spawn_blocking(move || {
                        let _ = w.process_one_job(&job);
                    })
                    .await;
                }
                Ok(None) => {}
                Err(e) => {
                    tracing::warn!("Render worker failed to acquire job: {}", e);
                }
            }

            tokio::select! {
                _ = tokio::time::sleep(poll_interval) => {}
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        break;
                    }
                }
            }
        }
    });

    LocalRenderWorkerHandle {
        shutdown_tx,
        join_handle,
    }
}
