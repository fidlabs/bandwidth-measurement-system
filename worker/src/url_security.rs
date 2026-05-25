use std::net::{IpAddr, Ipv4Addr};

use color_eyre::{
    eyre::{bail, Context, ContextCompat},
    Result,
};
use tokio::net::lookup_host;
use url::Url;

pub struct PublicHttpUrl {
    pub url: Url,
    pub ip_address: IpAddr,
}

pub async fn assert_public_http_url(url_str: &str) -> Result<Url> {
    Ok(resolve_public_http_url(url_str).await?.url)
}

pub async fn resolve_public_http_url(url_str: &str) -> Result<PublicHttpUrl> {
    let url = Url::parse(url_str).context("Invalid URL format")?;
    if url.scheme() != "http" && url.scheme() != "https" {
        bail!("URL scheme must be http or https");
    }

    let ip_address = validate_resolved_addresses_are_public(&url).await?;
    Ok(PublicHttpUrl { url, ip_address })
}

fn is_disallowed_ipv4(ip: Ipv4Addr) -> bool {
    ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_unspecified()
        || ip.is_multicast()
        || ip.octets()[0] == 0
        || (ip.octets()[0] == 100 && (ip.octets()[1] & 0b1100_0000) == 0b0100_0000)
}

fn is_disallowed_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_disallowed_ipv4(v4),
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_unique_local()
                || v6.is_unicast_link_local()
                || v6.is_multicast()
        }
    }
}

async fn validate_resolved_addresses_are_public(url: &Url) -> Result<IpAddr> {
    let host = url.host_str().context("URL host is missing")?;
    let port = url.port_or_known_default().context("URL port is missing")?;

    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_disallowed_ip(ip) {
            bail!("URL resolves to a blocked IP address");
        }
        return Ok(ip);
    }

    let resolved = lookup_host((host, port))
        .await
        .context("Failed to resolve URL host")?;

    let mut saw_address = false;
    let mut first_ip = None;
    for socket_addr in resolved {
        saw_address = true;
        first_ip.get_or_insert(socket_addr.ip());
        if is_disallowed_ip(socket_addr.ip()) {
            bail!("URL resolves to a blocked IP address");
        }
    }

    if !saw_address {
        bail!("URL host resolved to no addresses");
    }

    first_ip.context("URL host resolved to no addresses")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rejects_private_resolved_ips() {
        assert!(is_disallowed_ip("127.0.0.1".parse().unwrap()));
        assert!(is_disallowed_ip("10.0.0.1".parse().unwrap()));
        assert!(is_disallowed_ip("172.16.0.1".parse().unwrap()));
        assert!(is_disallowed_ip("192.168.1.1".parse().unwrap()));
        assert!(is_disallowed_ip("169.254.169.254".parse().unwrap()));
        assert!(is_disallowed_ip("100.64.0.1".parse().unwrap()));
        assert!(is_disallowed_ip("::1".parse().unwrap()));
    }

    #[test]
    fn test_allows_public_resolved_ips() {
        assert!(!is_disallowed_ip("8.8.8.8".parse().unwrap()));
        assert!(!is_disallowed_ip("1.1.1.1".parse().unwrap()));
        assert!(!is_disallowed_ip("2606:4700:4700::1111".parse().unwrap()));
    }

    #[tokio::test]
    async fn test_assert_public_http_url_rejects_literal_private_ip() {
        let result = assert_public_http_url("http://127.0.0.1/test").await;

        assert!(result.is_err(), "loopback URL should be rejected");
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("blocked IP address"));
    }

    #[tokio::test]
    async fn test_assert_public_http_url_allows_literal_public_ip() {
        assert_public_http_url("http://1.1.1.1/test")
            .await
            .expect("public literal IP should be allowed");
    }

    #[tokio::test]
    async fn test_resolve_public_http_url_returns_validated_ip() {
        let resolved = resolve_public_http_url("http://1.1.1.1/test")
            .await
            .expect("public literal IP should be allowed");

        assert_eq!(resolved.ip_address, "1.1.1.1".parse::<IpAddr>().unwrap());
    }

    #[tokio::test]
    async fn test_assert_public_http_url_rejects_non_http_scheme() {
        let result = assert_public_http_url("file:///etc/passwd").await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("scheme"));
    }
}
