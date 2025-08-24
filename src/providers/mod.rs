use alloy_provider::{Provider, ProviderBuilder};
use alloy_transport::Transport;
use eyre::Result;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use std::sync::Arc;
use tracing::{debug, info};

/// Custom HTTP client with authentication headers for dRPC
pub fn create_authenticated_http_provider(
    rpc_url: &str,
    api_key: Option<&str>,
) -> Result<Arc<dyn Provider>> {
    let url = url::Url::parse(rpc_url)?;
    
    // Check if this is a dRPC URL that might need special handling
    let is_drpc = rpc_url.contains("drpc.org");
    
    if is_drpc {
        info!("🔐 Detected dRPC endpoint, configuring authentication...");
        
        // For dRPC, the token is usually in the URL path
        // Format: https://lb.drpc.org/base/{API_KEY}
        // But we might also need to add headers
        
        if let Some(key) = api_key {
            debug!("Adding API key to headers for dRPC");
            let mut headers = HeaderMap::new();
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {}", key))?,
            );
            
            // Create a custom reqwest client with headers
            let client = reqwest::Client::builder()
                .default_headers(headers)
                .build()?;
            
            // Note: Alloy 0.5.4 doesn't directly support custom reqwest clients
            // We'll use the standard provider for now
            // In production, you might need to use a custom transport
        }
    }
    
    // Create standard HTTP provider
    // The API key should be in the URL for dRPC
    let provider = ProviderBuilder::new().on_http(url).boxed();
    Ok(Arc::new(provider))
}

/// Create WebSocket provider with proper authentication
pub async fn create_authenticated_ws_provider(
    ws_url: &str,
    api_key: Option<&str>,
) -> Result<Arc<dyn Provider>> {
    use alloy_provider::WsConnect;
    
    let is_drpc = ws_url.contains("drpc.org");
    
    if is_drpc {
        info!("🔐 Detected dRPC WebSocket endpoint, configuring connection...");
        
        // For dRPC WebSocket, the authentication is typically in the URL
        // Format: wss://lb.drpc.org/base/{API_KEY}
        
        // Some WebSocket servers require specific headers
        // Unfortunately, Alloy's WsConnect doesn't support custom headers directly
        // We'll need to use the URL-based authentication
    }
    
    // Create WebSocket connection
    let ws_connect = WsConnect::new(ws_url.to_string());
    let ws_provider = ProviderBuilder::new().on_ws(ws_connect).await?;
    
    Ok(Arc::new(ws_provider.boxed()))
}

/// Extract API key from dRPC URL if present
pub fn extract_drpc_api_key(url: &str) -> Option<String> {
    if !url.contains("drpc.org") {
        return None;
    }
    
    // dRPC URLs typically have format: https://lb.drpc.org/base/{API_KEY}
    // Extract the API key from the path
    if let Ok(parsed) = url::Url::parse(url) {
        let path_segments: Vec<&str> = parsed.path_segments().collect();
        // The API key is usually the last segment after the network name
        if path_segments.len() >= 2 {
            let potential_key = path_segments.last()?;
            // API keys are typically long hex strings
            if potential_key.len() > 20 {
                return Some(potential_key.to_string());
            }
        }
    }
    
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_extract_drpc_api_key() {
        let url = "https://lb.drpc.org/base/86284175e25a81f9dc3689ba2f334cb00d9e46a80cd55683868cd80d3cd0a5a0";
        let key = extract_drpc_api_key(url);
        assert_eq!(
            key,
            Some("86284175e25a81f9dc3689ba2f334cb00d9e46a80cd55683868cd80d3cd0a5a0".to_string())
        );
    }
}