use reqwest::Client;
use std::sync::LazyLock;
use std::time::Duration;

static HTTP_CLIENT: LazyLock<Client> = LazyLock::new(|| {
    Client::builder()
        .timeout(Duration::from_secs(120))
        .pool_max_idle_per_host(10)
        .build()
        .expect("Failed to build HTTP client")
});

pub fn client() -> &'static Client {
    &HTTP_CLIENT
}
