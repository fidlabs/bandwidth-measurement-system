use std::sync::Arc;

use axum::{debug_handler, extract::State, Json};
use axum_extra::extract::WithRejection;
use color_eyre::{
    eyre::{bail, Error},
    Result,
};
use common::api_response::{bad_request, ok_response, ApiResponse, ErrorResponse};
use futures::{stream, StreamExt};
use reqwest::Client;
use serde_json::json;
use tracing::debug;

use crate::AppState;

#[derive(serde::Deserialize)]
pub struct FindUrlInput {
    pub address: String,
    pub extended_search: Option<bool>,
}

#[derive(serde::Serialize)]
pub struct FindUrlResponse {
    pub url: String,
}

#[debug_handler]
pub async fn handle_find_url(
    State(state): State<Arc<AppState>>,
    WithRejection(Json(payload), _): WithRejection<Json<FindUrlInput>, ApiResponse<ErrorResponse>>,
) -> Result<ApiResponse<FindUrlResponse>, ApiResponse<()>> {
    // Get miner info from lotus rpc
    let peer_id = get_peer_id_from_lotus_rpc(&payload.address)
        .await
        .map_err(|e| {
            debug!("Failed to get peer id: {:?}", e);
            bad_request("Failed to get peer id")
        })?;

    // Get cid contact response
    let cid_contact_res = get_res_from_cid_contant(&peer_id).await.map_err(|e| {
        debug!("Missing data from cid contact: {:?}", e);
        bad_request("Missing data from cid contact")
    })?;
    debug!("cid contact response: {:?}", cid_contact_res);

    // Get all addresses (containing IP and Port) from cid contact response
    let addrs = get_all_addresses_from_cid_contact_response(cid_contact_res).map_err(|e| {
        debug!("Failed to get addresses: {:?}", e);
        bad_request("Failed to get addresses")
    })?;

    // parse addresses to http endpoints
    let endpoints = parse_addrs_to_endpoints(addrs).map_err(|e| {
        debug!("Failed to parse addrs: {:?}", e);
        bad_request("Failed to parse addrs")
    })?;

    if endpoints.is_empty() {
        debug!("No endpoints found");
        return Err(bad_request("No endpoints found"));
    }

    let provider = payload
        .address
        .strip_prefix("f0")
        .unwrap_or(&payload.address)
        .to_string();

    // Find piece_cid from deals and test if the url is working, returning the first working url
    let working_url = get_working_url(state, endpoints, &provider, payload.extended_search)
        .await
        .map_err(|e| {
            debug!("Failed to get working url: {:?}", e);
            bad_request("Failed to get working url")
        })?;

    Ok(ok_response(FindUrlResponse { url: working_url }))
}

async fn get_peer_id_from_lotus_rpc(address: &str) -> Result<String, Error> {
    let client = Client::new();
    let res = client
        .post("https://api.node.glif.io/rpc/v1")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "Filecoin.StateMinerInfo",
            "params": [address, null]
        }))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    let peer_id = res
        .get("result")
        .unwrap()
        .get("PeerId")
        .unwrap()
        .as_str()
        .unwrap();

    Ok(peer_id.to_string())
}

async fn get_res_from_cid_contant(peer_id: &str) -> Result<serde_json::Value> {
    let client = Client::new();
    let url = format!("https://cid.contact/providers/{}", peer_id);

    debug!("cid contact url: {:?}", url);

    let res = client
        .get(&url)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    Ok(res)
}

fn get_all_addresses_from_cid_contact_response(json: serde_json::Value) -> Result<Vec<String>> {
    let mut addresses = vec![];

    // let keys = ["AddrInfo", "Publisher"];
    // for key in &keys {
    //     if let Some(addrs_arr) = json
    //         .get(*key)
    //         .and_then(|item| item.get("Addrs"))
    //         .and_then(|addrs| addrs.as_array())
    //     {
    //         addrs_arr
    //             .iter()
    //             .filter_map(|addr| addr.as_str())
    //             .for_each(|addr_str| addresses.push(addr_str.to_string()));
    //     }
    // }

    if let Some(e_providers) = json
        .get("ExtendedProviders")
        .and_then(|ep| ep.get("Providers"))
        .and_then(|p| p.as_array())
    {
        e_providers
            .iter()
            .filter_map(|provider| provider.get("Addrs"))
            .filter_map(|addrs| addrs.as_array())
            .flat_map(|addrs_arr| addrs_arr.iter())
            .filter_map(|addr| addr.as_str())
            .for_each(|addr| {
                addresses.push(addr.to_string());
            });
    }

    if addresses.is_empty() {
        bail!("No addresses found");
    }

    Ok(addresses)
}

fn parse_addrs_to_endpoints(addrs: Vec<String>) -> Result<Vec<String>, Error> {
    let mut endpoints = vec![];

    for addr in addrs {
        let parts: Vec<&str> = addr.split("/").collect();
        let prot = if addr.contains("https") {
            "https"
        } else {
            "http"
        };
        let host = parts[2];
        let port = parts[4];

        let endpoint = format!("{}://{}:{}", prot, host, port);

        if !addr.contains("http") {
            debug!("skipping non-http endpoint: {:?}", endpoint);
            continue;
        }

        endpoints.push(endpoint);
    }

    Ok(endpoints)
}

/// return first working url
async fn filter_working_url(urls: Vec<String>) -> Option<String> {
    let client = Client::new();

    // Create a stream of requests testing the urls through head requests
    // Run the requests concurrently with a limit
    let mut stream = stream::iter(urls)
        .map(|url| {
            let client = client.clone();
            async move {
                match client.head(&url).send().await {
                    Ok(resp) if resp.status().is_success() => Some(url),
                    _ => {
                        debug!("url not working: {:?}", url);
                        None
                    }
                }
            }
        })
        .buffer_unordered(20); // concurency limit

    while let Some(result) = stream.next().await {
        if let Some(url) = result {
            return Some(url);
        }
    }

    None
}

async fn get_working_url(
    state: Arc<AppState>,
    endpoints: Vec<String>,
    provider: &str,
    extended_search: Option<bool>,
) -> Result<String, Error> {
    let limit = 1000;
    let mut offset = 0;
    let max_offset = if extended_search.unwrap_or(false) {
        5 * limit
    } else {
        limit
    };

    loop {
        let deals = state
            .deal_repo
            .get_unified_verified_deals_by_provider(provider, limit, offset)
            .await?;

        if deals.is_empty() {
            break;
        }

        debug!("number of deals: {:?}", deals.len());

        // construct every piece_cid and endoint combination
        let urls: Vec<String> = endpoints
            .iter()
            .flat_map(|endpoint| {
                let endpoint = endpoint.clone();
                deals.iter().filter_map(move |deal| {
                    deal.piece_cid
                        .as_ref()
                        .map(|piece_cid| format!("{}/piece/{}", endpoint, piece_cid))
                })
            })
            .collect();

        let working_url = filter_working_url(urls).await;

        if working_url.is_some() {
            debug!("working url found: {:?}", working_url);
            return Ok(working_url.unwrap());
        }

        offset += limit;
        if offset >= max_offset {
            break;
        }
        debug!("No working url found, fetching more deals");
    }

    bail!("No working url found")
}
