use once_cell::sync::Lazy;
use reqwest::Client;
use std::time::Duration;

/// Global reusable HTTP client — eliminates DNS + TLS handshake per request.
static HTTP_CLIENT: Lazy<Client> = Lazy::new(|| {
    Client::builder()
        .timeout(Duration::from_secs(120))
        .pool_max_idle_per_host(10)
        .build()
        .expect("Failed to build HTTP client")
});

pub fn client() -> &'static Client {
    &HTTP_CLIENT
}
