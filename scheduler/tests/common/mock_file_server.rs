// scheduler/tests/common/mock_file_server.rs

use std::time::Duration;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

pub struct MockFileServer {
    server: MockServer,
}

impl MockFileServer {
    pub async fn start() -> Self {
        Self {
            server: MockServer::start().await,
        }
    }

    pub fn url(&self, file_path: &str) -> String {
        format!("{}{}", self.server.uri(), file_path)
    }

    pub fn uri(&self) -> String {
        self.server.uri()
    }

    /// Setup a file endpoint with optional TTFB delay
    /// Simple version: returns fixed-size response
    pub async fn setup_file(
        &self,
        file_path: &str,
        size_bytes: usize,
        ttfb_delay: Option<Duration>,
    ) {
        let body = vec![0u8; size_bytes];

        let mut response = ResponseTemplate::new(200)
            .insert_header("content-length", size_bytes.to_string())
            .insert_header("accept-ranges", "bytes")
            .set_body_bytes(body.clone());

        if let Some(delay) = ttfb_delay {
            response = response.set_delay(delay);
        }

        // Mount GET handler
        Mock::given(method("GET"))
            .and(path(file_path))
            .respond_with(response.clone())
            .mount(&self.server)
            .await;

        // Mount HEAD handler (for URL validation)
        let head_response = ResponseTemplate::new(200)
            .insert_header("content-length", size_bytes.to_string())
            .insert_header("accept-ranges", "bytes");

        Mock::given(method("HEAD"))
            .and(path(file_path))
            .respond_with(head_response)
            .mount(&self.server)
            .await;
    }

    /// Setup a file that returns an error
    pub async fn setup_file_error(&self, file_path: &str, status_code: u16) {
        let response = ResponseTemplate::new(status_code);

        Mock::given(method("GET"))
            .and(path(file_path))
            .respond_with(response.clone())
            .mount(&self.server)
            .await;

        Mock::given(method("HEAD"))
            .and(path(file_path))
            .respond_with(response)
            .mount(&self.server)
            .await;
    }
}
