use std::sync::Arc;
use std::sync::atomic::AtomicU8;
use std::sync::atomic::Ordering;

use futures::lock::Mutex;
use tokio_util::sync::CancellationToken;

use crate::Extension;
use crate::ExtensionConfig;
use crate::ExtensionError;
use crate::ExtensionRequest;
use crate::ExtensionResponse;

const MAX_RESTARTS: u8 = 3;

pub struct ManagedExtension {
    config: ExtensionConfig,
    process: Mutex<Option<Arc<Extension>>>,
    failures: AtomicU8,
    restored: Mutex<serde_json::Value>,
}

impl ManagedExtension {
    pub async fn launch(config: ExtensionConfig) -> Result<Self, ExtensionError> {
        let process = Extension::launch(config.clone()).await?;
        Ok(Self {
            restored: Mutex::new(config.state.clone()),
            config,
            process: Mutex::new(Some(Arc::new(process))),
            failures: AtomicU8::new(0),
        })
    }

    pub async fn manifest(&self) -> Result<antex_extension_protocol::Manifest, ExtensionError> {
        Ok(self.process().await?.manifest().clone())
    }

    pub async fn request(
        &self,
        request: ExtensionRequest,
        cancellation: CancellationToken,
    ) -> Result<ExtensionResponse, ExtensionError> {
        let process = self.process().await?;
        let result = process.request(request, cancellation).await;
        match result {
            Ok(response) => {
                self.failures.store(0, Ordering::Relaxed);
                Ok(response)
            }
            Err(error) => {
                if error.is_fatal() {
                    let mut current = self.process.lock().await;
                    if current
                        .as_ref()
                        .is_some_and(|current| Arc::ptr_eq(current, &process))
                    {
                        *current = None;
                        self.failures.fetch_add(1, Ordering::Relaxed);
                    }
                }
                Err(error)
            }
        }
    }

    pub async fn shutdown(&self) -> Result<(), ExtensionError> {
        let process = self.process.lock().await.take();
        match process {
            Some(process) => process.shutdown().await,
            None => Ok(()),
        }
    }

    pub async fn reset(&self, state: serde_json::Value) {
        *self.restored.lock().await = state;
        if let Some(process) = self.process.lock().await.take() {
            process.terminate().await;
        }
        self.failures.store(0, Ordering::Relaxed);
    }

    async fn process(&self) -> Result<Arc<Extension>, ExtensionError> {
        let mut process = self.process.lock().await;
        if let Some(process) = process.as_ref() {
            return Ok(Arc::clone(process));
        }
        let failures = self.failures.load(Ordering::Relaxed);
        if failures >= MAX_RESTARTS {
            return Err(ExtensionError::RestartLimit);
        }
        let mut config = self.config.clone();
        config.state = self.restored.lock().await.clone();
        match Extension::launch(config).await {
            Ok(restarted) => {
                let restarted = Arc::new(restarted);
                *process = Some(Arc::clone(&restarted));
                Ok(restarted)
            }
            Err(error) => {
                self.failures.fetch_add(1, Ordering::Relaxed);
                Err(error)
            }
        }
    }
}
