mod auth;
mod body;
mod browser;
mod continuation;
mod http;
mod limits;
mod login;
mod provider;
mod storage;
mod wire;

pub use browser::BrowserLogin;
pub use login::DeviceLogin;
pub use provider::OpenAiProvider;

#[cfg(test)]
#[path = "provider_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "login_tests.rs"]
mod login_tests;

#[cfg(all(test, target_os = "linux"))]
mod tls_tests;
