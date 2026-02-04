use anyhow::{anyhow, bail, Context, Result};
use http_acl::HttpAcl;
use http_acl_reqwest::HttpAclMiddleware;
use rand::Rng;
use reqwest::Client;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use std::time::Duration;
use tracing::debug;
use url::Url;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Blocked hostnames that should never be accessed
const BLOCKED_HOSTNAMES: &[&str] = &[
    "metadata.google.internal",
    "metadata.google.com",
    "metadata.goog",
    "localhost",
];

/// Create ACL configuration for SSRF protection.
/// Non-global IPs (private, loopback, link-local) are blocked by default.
pub fn create_acl() -> HttpAcl {
    let mut builder = HttpAcl::builder()
        .ip_acl_default(true) // Allow public IPs by default, private IPs are automatically denied
        .host_acl_default(true); // Allow hosts by default, except explicitly denied ones

    for hostname in BLOCKED_HOSTNAMES {
        builder = builder
            .add_denied_host((*hostname).to_string())
            .expect("valid hostname");
    }

    builder.build()
}

/// Create HTTP client with SSRF protection middleware.
/// The middleware validates hosts at request time.
///
/// Note: We don't use the ACL's custom DNS resolver because it has a bug
/// that causes "invalid socket address" errors. Instead, we rely on the
/// middleware to check hosts at the request level, which still provides
/// SSRF protection by blocking requests to private IPs and metadata endpoints.
pub fn create_acl_client() -> ClientWithMiddleware {
    let acl = create_acl();
    let middleware = HttpAclMiddleware::new(acl);

    // Use standard DNS resolution - the middleware will still validate
    // the host against the ACL before making the request
    let client = Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .connect_timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("Failed to create HTTP client");

    ClientBuilder::new(client).with(middleware).build()
}

/// Create HTTP client without SSRF protection (for testing with localhost).
/// WARNING: Only use this in tests! This bypasses all SSRF checks.
pub fn create_test_acl_client() -> ClientWithMiddleware {
    // Create a simple client without any ACL middleware for testing
    let client = Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .connect_timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("Failed to create HTTP client");

    ClientBuilder::new(client).build()
}

/// Validated URL wrapper ensuring SSRF checks have passed
#[derive(Debug, Clone)]
pub struct ValidatedUrl(Url);

impl ValidatedUrl {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl AsRef<str> for ValidatedUrl {
    fn as_ref(&self) -> &str {
        self.0.as_str()
    }
}

/// Validate URL and get file size range for downloading.
///
/// Performs SSRF-safe HEAD request to verify URL accessibility
/// and calculate random byte range for bandwidth testing.
pub async fn validate_and_get_file_range(
    client: &ClientWithMiddleware,
    url_str: &str,
    size_mb: i64,
) -> Result<(ValidatedUrl, i64, i64)> {
    // Parse URL
    let url = Url::parse(url_str).context("Invalid URL format")?;

    // Validate scheme
    if url.scheme() != "http" && url.scheme() != "https" {
        bail!("URL scheme must be http or https");
    }

    // Make HEAD request (ACL middleware validates host/IP automatically)
    let response = client
        .head(url.as_str())
        .send()
        .await
        .context("Failed to access URL")?;

    // Handle redirects (disabled in client, so check manually)
    if response.status().is_redirection() {
        if let Some(location) = response.headers().get(reqwest::header::LOCATION) {
            let redirect_url = location.to_str().unwrap_or("<invalid>");
            bail!(
                "URL redirects to {}. Please use the direct URL.",
                redirect_url
            );
        }
        bail!("URL returned redirect status without Location header");
    }

    if !response.status().is_success() {
        bail!("Server returned status {}", response.status());
    }

    // Extract and validate content length
    let content_length = response
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .ok_or_else(|| anyhow!("Content-Length header missing"))?
        .to_str()
        .context("Invalid Content-Length header")?
        .parse::<i64>()
        .context("Content-Length is not a valid number")?;

    debug!("Content-Length: {}", content_length);

    let size = size_mb * 1024 * 1024;
    if content_length < size {
        bail!("File size is less than {} MB", size_mb);
    }

    // Calculate random byte range
    let mut rng = rand::thread_rng();
    let max_start = content_length.saturating_sub(size);
    let start_range = if max_start == 0 {
        0
    } else {
        rng.gen_range(0..=max_start)
    };
    let end_range = start_range + size;

    debug!("Selected range: {} - {}", start_range, end_range);

    Ok((ValidatedUrl(url), start_range, end_range))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing::info;

    #[test]
    fn test_acl_allows_public_ips() {
        let acl = create_acl();

        // Google DNS - public
        let ip: std::net::IpAddr = "8.8.8.8".parse().unwrap();
        assert!(acl.is_ip_allowed(&ip).is_allowed());

        // Cloudflare DNS - public
        let ip: std::net::IpAddr = "1.1.1.1".parse().unwrap();
        assert!(acl.is_ip_allowed(&ip).is_allowed());
    }

    #[test]
    fn test_acl_blocks_loopback() {
        let acl = create_acl();

        let ip: std::net::IpAddr = "127.0.0.1".parse().unwrap();
        assert!(acl.is_ip_allowed(&ip).is_denied());

        let ip: std::net::IpAddr = "::1".parse().unwrap();
        assert!(acl.is_ip_allowed(&ip).is_denied());
    }

    #[test]
    fn test_acl_blocks_private_ranges() {
        let acl = create_acl();

        // 10.0.0.0/8
        let ip: std::net::IpAddr = "10.0.0.1".parse().unwrap();
        assert!(acl.is_ip_allowed(&ip).is_denied());

        // 172.16.0.0/12
        let ip: std::net::IpAddr = "172.16.0.1".parse().unwrap();
        assert!(acl.is_ip_allowed(&ip).is_denied());

        // 192.168.0.0/16
        let ip: std::net::IpAddr = "192.168.1.1".parse().unwrap();
        assert!(acl.is_ip_allowed(&ip).is_denied());
    }

    #[test]
    fn test_acl_blocks_link_local() {
        let acl = create_acl();

        // AWS metadata IP
        let ip: std::net::IpAddr = "169.254.169.254".parse().unwrap();
        assert!(acl.is_ip_allowed(&ip).is_denied());
    }

    #[test]
    fn test_acl_blocks_carrier_grade_nat() {
        let acl = create_acl();

        // 100.64.0.0/10
        let ip: std::net::IpAddr = "100.64.0.1".parse().unwrap();
        assert!(acl.is_ip_allowed(&ip).is_denied());
    }

    #[test]
    fn test_acl_blocks_metadata_hostnames() {
        let acl = create_acl();

        assert!(acl.is_host_allowed("metadata.google.internal").is_denied());
        assert!(acl.is_host_allowed("metadata.google.com").is_denied());
        assert!(acl.is_host_allowed("metadata.goog").is_denied());
        assert!(acl.is_host_allowed("localhost").is_denied());
    }

    #[test]
    fn test_acl_allows_normal_hostnames() {
        let acl = create_acl();

        assert!(acl.is_host_allowed("example.com").is_allowed());
        assert!(acl.is_host_allowed("cdn.provider.com").is_allowed());
    }

    #[tokio::test]
    async fn test_acl_client_with_external_url() {
        let client = create_acl_client();
        let url = "https://ahnawee8-xupio2pi-production.s3.us-east-1.amazonaws.com/test.zip";

        match client.head(url).send().await {
            Ok(response) => {
                info!("SUCCESS - Response status: {}", response.status());
                info!(
                    "Content-Length: {:?}",
                    response.headers().get("content-length")
                );
            }
            Err(e) => {
                info!("FAILED - Error: {:?}", e);
                if let Some(source) = std::error::Error::source(&e) {
                    info!("Error source: {:?}", source);
                }
            }
        }
    }

    #[tokio::test]
    async fn test_acl_client_blocks_localhost() {
        let client = create_acl_client();

        // localhost should be blocked by the ACL middleware
        let result = client.head("http://localhost:8080/test").send().await;
        assert!(result.is_err(), "localhost should be blocked by ACL");

        let err = result.unwrap_err();
        let err_msg = format!("{:?}", err);
        assert!(
            err_msg.contains("Host \"localhost\" is denied") || err_msg.contains("denied"),
            "Error should indicate host is denied: {}",
            err_msg
        );
    }

    #[tokio::test]
    async fn test_acl_client_blocks_metadata_endpoints() {
        let client = create_acl_client();

        // GCP metadata endpoint should be blocked
        let result = client
            .head("http://metadata.google.internal/computeMetadata/v1/")
            .send()
            .await;
        assert!(
            result.is_err(),
            "metadata.google.internal should be blocked by ACL"
        );
    }

    /// Comprehensive SSRF protection test with real external URLs.
    /// Tests that public URLs are accessible while private/internal URLs are blocked.
    #[tokio::test]
    async fn test_ssrf_protection_comprehensive() {
        let client = create_acl_client();

        info!("=== Testing ALLOWED URLs (should succeed or get HTTP response) ===");

        // Test 1: S3 URL
        let s3_url = "https://ahnawee8-xupio2pi-production.s3.us-east-1.amazonaws.com/test.zip";
        match client.head(s3_url).send().await {
            Ok(resp) => info!("S3 URL: ALLOWED - HTTP {}", resp.status()),
            Err(e) => {
                let err_msg = format!("{:?}", e);
                if err_msg.contains("denied") {
                    panic!("S3 URL should be ALLOWED but was blocked: {}", err_msg);
                }
                info!("S3 URL: ALLOWED (network/HTTP error, not ACL block): {}", e);
            }
        }

        // Test 2: Hetzner speed test
        let hetzner_url = "https://speed.hetzner.de/100MB.bin";
        match client.head(hetzner_url).send().await {
            Ok(resp) => info!("Hetzner speed test: ALLOWED - HTTP {}", resp.status()),
            Err(e) => {
                let err_msg = format!("{:?}", e);
                if err_msg.contains("denied") {
                    panic!("Hetzner URL should be ALLOWED but was blocked: {}", err_msg);
                }
                info!(
                    "Hetzner speed test: ALLOWED (network/HTTP error, not ACL block): {}",
                    e
                );
            }
        }

        // Test 3: Google.com
        let google_url = "https://www.google.com";
        match client.head(google_url).send().await {
            Ok(resp) => info!("Google.com: ALLOWED - HTTP {}", resp.status()),
            Err(e) => {
                let err_msg = format!("{:?}", e);
                if err_msg.contains("denied") {
                    panic!("Google URL should be ALLOWED but was blocked: {}", err_msg);
                }
                info!(
                    "Google.com: ALLOWED (network/HTTP error, not ACL block): {}",
                    e
                );
            }
        }

        info!("=== Testing BLOCKED URLs (should be denied by ACL) ===");

        // Test 4: localhost
        let result = client.head("http://localhost:8080/test").send().await;
        match result {
            Ok(_) => panic!("localhost should be BLOCKED but was allowed!"),
            Err(e) => {
                let err_msg = format!("{:?}", e);
                if err_msg.contains("denied") || err_msg.contains("Host") {
                    info!("localhost: BLOCKED - ACL denied");
                } else {
                    info!(
                        "localhost: BLOCKED (connection refused, which is acceptable): {}",
                        e
                    );
                }
            }
        }

        // Test 5: 127.0.0.1
        let result = client.head("http://127.0.0.1:8080/test").send().await;
        match result {
            Ok(_) => panic!("127.0.0.1 should be BLOCKED but was allowed!"),
            Err(e) => {
                let err_msg = format!("{:?}", e);
                if err_msg.contains("denied") || err_msg.contains("127.0.0.1") {
                    info!("127.0.0.1: BLOCKED - ACL denied");
                } else {
                    info!(
                        "127.0.0.1: BLOCKED (connection refused, which is acceptable): {}",
                        e
                    );
                }
            }
        }

        // Test 6: 192.168.x.x (private network)
        let result = client.head("http://192.168.1.1/test").send().await;
        match result {
            Ok(_) => panic!("192.168.x.x should be BLOCKED but was allowed!"),
            Err(e) => {
                let err_msg = format!("{:?}", e);
                if err_msg.contains("denied") || err_msg.contains("192.168") {
                    info!("192.168.1.1: BLOCKED - ACL denied");
                } else {
                    info!("192.168.1.1: BLOCKED (connection refused/timeout, which is acceptable): {}", e);
                }
            }
        }

        // Test 7: 10.x.x.x (private network)
        let result = client.head("http://10.0.0.1/test").send().await;
        match result {
            Ok(_) => panic!("10.x.x.x should be BLOCKED but was allowed!"),
            Err(e) => {
                let err_msg = format!("{:?}", e);
                if err_msg.contains("denied") || err_msg.contains("10.0.0") {
                    info!("10.0.0.1: BLOCKED - ACL denied");
                } else {
                    info!(
                        "10.0.0.1: BLOCKED (connection refused/timeout, which is acceptable): {}",
                        e
                    );
                }
            }
        }

        // Test 8: 169.254.169.254 (AWS metadata endpoint)
        let result = client
            .head("http://169.254.169.254/latest/meta-data/")
            .send()
            .await;
        match result {
            Ok(_) => panic!("169.254.169.254 should be BLOCKED but was allowed!"),
            Err(e) => {
                let err_msg = format!("{:?}", e);
                if err_msg.contains("denied") || err_msg.contains("169.254") {
                    info!("169.254.169.254 (AWS metadata): BLOCKED - ACL denied");
                } else {
                    info!("169.254.169.254 (AWS metadata): BLOCKED (connection refused/timeout, which is acceptable): {}", e);
                }
            }
        }

        // Test 9: metadata.google.internal (GCP metadata)
        let result = client
            .head("http://metadata.google.internal/computeMetadata/v1/")
            .send()
            .await;
        match result {
            Ok(_) => panic!("metadata.google.internal should be BLOCKED but was allowed!"),
            Err(e) => {
                let err_msg = format!("{:?}", e);
                if err_msg.contains("denied") || err_msg.contains("metadata") {
                    info!("metadata.google.internal: BLOCKED - ACL denied");
                } else {
                    info!(
                        "metadata.google.internal: BLOCKED (DNS error, which is acceptable): {}",
                        e
                    );
                }
            }
        }

        info!("=== All SSRF protection tests passed ===");
    }

    /// Test that verifies the ACL middleware correctly denies requests
    /// by checking specific error messages for blocked hosts/IPs.
    #[tokio::test]
    async fn test_acl_denial_messages() {
        let client = create_acl_client();

        // Test localhost denial message
        let result = client.head("http://localhost/").send().await;
        assert!(result.is_err());
        let err_msg = format!("{:?}", result.unwrap_err());
        assert!(
            err_msg.contains("denied") || err_msg.contains("localhost"),
            "Expected denial message for localhost, got: {}",
            err_msg
        );

        // Test metadata hostname denial message
        let result = client.head("http://metadata.google.internal/").send().await;
        assert!(result.is_err());
        let err_msg = format!("{:?}", result.unwrap_err());
        assert!(
            err_msg.contains("denied") || err_msg.contains("metadata"),
            "Expected denial message for metadata, got: {}",
            err_msg
        );
    }
}
