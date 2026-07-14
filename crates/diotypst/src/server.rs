use crate::{DocumentWorkspace, DownloadFormat, RenderEnvironment};
#[cfg(feature = "server")]
use crate::{DownloadError, RenderDownloadError, render_download};

/// A server-side render request that prepares one Download File.
///
/// The Typst Project and Render Environment are carried as the domain types directly;
/// with the `serde` feature, deserialization validates them (Project Paths, duplicate
/// files, root entrypoint, duplicate package specs) before a request ever reaches
/// rendering. Rendering and packaging go through [`crate::render_download`], so the Server
/// Render Route and client-side Download Actions produce identical bytes.
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Clone, Debug, PartialEq)]
pub struct ServerRenderRequest {
    workspace: DocumentWorkspace,
    environment: RenderEnvironment,
    format: DownloadFormat,
    filename: String,
}

impl ServerRenderRequest {
    /// Create a server-side render request.
    pub fn new(
        workspace: DocumentWorkspace,
        environment: RenderEnvironment,
        format: DownloadFormat,
        filename: impl Into<String>,
    ) -> Self {
        Self {
            workspace,
            environment,
            format,
            filename: filename.into(),
        }
    }

    /// Return the requested Typst Project.
    pub fn workspace(&self) -> &DocumentWorkspace {
        &self.workspace
    }

    /// Return the requested Render Environment.
    pub fn environment(&self) -> &RenderEnvironment {
        &self.environment
    }

    /// Return the requested download format.
    pub fn format(&self) -> DownloadFormat {
        self.format
    }

    /// Return the suggested download filename.
    pub fn filename(&self) -> &str {
        &self.filename
    }
}

/// Build an HTTP download response for a Server Render Route request.
#[cfg(feature = "server")]
pub fn server_render_download_response(
    request: &ServerRenderRequest,
) -> Result<axum::response::Response, RenderDownloadError> {
    let file = render_download(
        &request.workspace,
        &request.environment,
        request.format,
        request.filename.clone(),
    )?;
    let mut response = axum::response::Response::new(axum::body::Body::from(file.bytes().to_vec()));

    response.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_str(file.media_type())
            .expect("download media type should be a valid header value"),
    );
    response.headers_mut().insert(
        axum::http::header::CONTENT_DISPOSITION,
        axum::http::HeaderValue::from_str(&attachment_content_disposition(file.filename()))
            .expect("sanitized content disposition should be a valid header value"),
    );

    Ok(response)
}

/// Default path for the Server Render Route download endpoint.
#[cfg(feature = "server")]
pub const SERVER_RENDER_DOWNLOAD_PATH: &str = "/typst/render-download";

/// Build an Axum router exposing the Server Render Route download endpoint.
#[cfg(feature = "server")]
pub fn server_render_download_router() -> axum::Router {
    axum::Router::new().route(
        SERVER_RENDER_DOWNLOAD_PATH,
        axum::routing::post(server_render_download_handler),
    )
}

/// Axum JSON handler for the Server Render Route download endpoint.
#[cfg(feature = "server")]
pub async fn server_render_download_handler(
    axum::Json(request): axum::Json<ServerRenderRequest>,
) -> Result<axum::response::Response, RenderDownloadError> {
    server_render_download_response(&request)
}

#[cfg(feature = "server")]
impl axum::response::IntoResponse for RenderDownloadError {
    fn into_response(self) -> axum::response::Response {
        let status = match &self {
            Self::Render(_) => axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            Self::Download(DownloadError::UnsupportedArtifact) => {
                axum::http::StatusCode::UNSUPPORTED_MEDIA_TYPE
            }
            Self::Download(DownloadError::Unavailable) => axum::http::StatusCode::NOT_FOUND,
        };

        (status, format!("{self:?}")).into_response()
    }
}

#[cfg(feature = "server")]
fn attachment_content_disposition(filename: &str) -> String {
    let filename = filename
        .chars()
        .map(|character| match character {
            '!' | '#'..='[' | ']'..='~' => character,
            _ => '_',
        })
        .collect::<String>();

    format!("attachment; filename=\"{filename}\"")
}
