use crate::artifacts::repository::ArtifactRepository;
use crate::artifacts::resolver::ArtifactResolver;
use rusqlite::Connection;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

pub struct PreviewResponse {
    pub status_code: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl PreviewResponse {
    pub fn into_http_response(self) -> Result<tauri::http::Response<Vec<u8>>, String> {
        let mut builder = tauri::http::Response::builder().status(self.status_code);
        for (k, v) in self.headers {
            builder = builder.header(k, v);
        }
        builder.body(self.body).map_err(|e| e.to_string())
    }
}

pub fn handle_preview_range_request(
    artifact_id: &str,
    range_header: Option<&str>,
    resolver: &ArtifactResolver,
    conn: &Connection,
) -> Result<PreviewResponse, String> {
    // 1. Path traversal & security check
    if artifact_id.is_empty()
        || artifact_id.contains('/')
        || artifact_id.contains('\\')
        || artifact_id.contains("..")
    {
        return Ok(PreviewResponse {
            status_code: 400,
            headers: vec![("Content-Type".to_string(), "text/plain".to_string())],
            body: b"Invalid artifact ID".to_vec(),
        });
    }

    // 2. Resolve artifact in DB
    let artifact = match ArtifactRepository::get_by_id(conn, artifact_id) {
        Ok(Some(a)) => a,
        Ok(None) => {
            return Ok(PreviewResponse {
                status_code: 404,
                headers: vec![("Content-Type".to_string(), "text/plain".to_string())],
                body: b"Artifact Not Found".to_vec(),
            });
        }
        Err(e) => return Err(e.to_string()),
    };

    // 3. Resolve canonical file path
    let canonical_path = match resolver.resolve_uri(&format!("artifact://{}", artifact_id)) {
        Ok(p) => p,
        Err(_) => {
            return Ok(PreviewResponse {
                status_code: 404,
                headers: vec![("Content-Type".to_string(), "text/plain".to_string())],
                body: b"Storage File Missing".to_vec(),
            });
        }
    };

    let mut file = match File::open(&canonical_path) {
        Ok(f) => f,
        Err(_) => {
            return Ok(PreviewResponse {
                status_code: 404,
                headers: vec![("Content-Type".to_string(), "text/plain".to_string())],
                body: b"Unable to open file".to_vec(),
            });
        }
    };

    let file_len = match file.metadata() {
        Ok(m) => m.len(),
        Err(_) => return Err("Failed to read file metadata".to_string()),
    };

    let mime_type = artifact
        .mime_type
        .unwrap_or_else(|| "application/octet-stream".to_string());

    // 4. Handle Range header
    if let Some(range_str) = range_header {
        if let Some(spec) = range_str.strip_prefix("bytes=") {
            let parts: Vec<&str> = spec.split('-').collect();
            if parts.len() == 2 {
                let start_res = parts[0].parse::<u64>();
                let end_res = if parts[1].is_empty() {
                    None
                } else {
                    parts[1].parse::<u64>().ok()
                };

                if let Ok(start) = start_res {
                    if start >= file_len {
                        return Ok(PreviewResponse {
                            status_code: 416,
                            headers: vec![
                                ("Content-Range".to_string(), format!("bytes */{}", file_len)),
                                ("Content-Type".to_string(), mime_type),
                            ],
                            body: Vec::new(),
                        });
                    }

                    let end = end_res.unwrap_or(file_len - 1).min(file_len - 1);
                    if start <= end {
                        let length = end - start + 1;
                        if file.seek(SeekFrom::Start(start)).is_err() {
                            return Err("Seek failed".to_string());
                        }

                        let mut buffer = vec![0u8; length as usize];
                        if file.read_exact(&mut buffer).is_err() {
                            return Err("Read failed".to_string());
                        }

                        return Ok(PreviewResponse {
                            status_code: 206,
                            headers: vec![
                                ("Content-Type".to_string(), mime_type),
                                (
                                    "Content-Range".to_string(),
                                    format!("bytes {}-{}/{}", start, end, file_len),
                                ),
                                ("Content-Length".to_string(), length.to_string()),
                                ("Accept-Ranges".to_string(), "bytes".to_string()),
                                ("Access-Control-Allow-Origin".to_string(), "*".to_string()),
                            ],
                            body: buffer,
                        });
                    }
                }
            }
        }
    }

    // Full 200 OK
    let mut buffer = Vec::with_capacity(file_len as usize);
    if file.read_to_end(&mut buffer).is_err() {
        return Err("Read to end failed".to_string());
    }

    Ok(PreviewResponse {
        status_code: 200,
        headers: vec![
            ("Content-Type".to_string(), mime_type),
            ("Content-Length".to_string(), file_len.to_string()),
            ("Accept-Ranges".to_string(), "bytes".to_string()),
            ("Access-Control-Allow-Origin".to_string(), "*".to_string()),
        ],
        body: buffer,
    })
}
