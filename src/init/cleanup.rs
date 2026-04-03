//! Cleanup registry — mirrors TypeScript's utils/cleanupRegistry.ts
//!
//! Functions registered here run in reverse order during graceful shutdown.
//! Used for: analytics flush, LSP teardown, session save, plugin cleanup.
//!
//! Key insight: FnOnce is not Sync, so we wrap each cleanup in Arc<Mutex<Option<...>>>.
//! Arc is Sync, Mutex is Sync, so the registry can safely store them in a OnceLock.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

/// An async cleanup function. Fn (not FnOnce) so the registry can be Sync.
type AsyncCleanupFn =
    Box<dyn Fn() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + 'static>;

/// A registered cleanup entry: Arc so it's Sync (needed for OnceLock), Mutex so
/// we can take() the Fn and run it once.
type CleanupEntry = Arc<Mutex<Option<AsyncCleanupFn>>>;

static REGISTRY: std::sync::OnceLock<Mutex<Vec<CleanupEntry>>> =
    std::sync::OnceLock::new();

fn with_registry<F, T>(f: F) -> T
where
    F: FnOnce(&mut Vec<CleanupEntry>) -> T,
{
    let registry = REGISTRY.get_or_init(|| Mutex::new(Vec::new()));
    let mut guard = registry.lock().unwrap();
    f(&mut guard)
}

/// Register a cleanup function to run on graceful shutdown.
/// Returns an unregister handle (removes from registry on call).
pub fn register_cleanup<F>(f: F) -> impl FnOnce() + Send + 'static
where
    F: Fn() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + 'static + Sized,
{
    let entry: CleanupEntry = Arc::new(Mutex::new(Some(Box::new(f))));
    let id_ptr = Arc::as_ptr(&entry) as usize;

    with_registry(|vec| {
        vec.push(entry);
    });

    move || {
        with_registry(|vec| {
            vec.retain(|e| Arc::as_ptr(e) as usize != id_ptr);
        });
    }
}

/// Run all registered cleanup functions concurrently.
/// Called by graceful_shutdown(). Mirrors TypeScript's runCleanupFunctions().
pub async fn run_cleanup_functions() {
    let entries: Vec<_> = with_registry(|vec| vec.clone());

    if entries.is_empty() {
        return;
    }

    // Run each cleanup: take the Fn out of the Option, invoke it.
    let futures: Vec<_> = entries
        .into_iter()
        .filter_map(|entry| {
            let mut guard = entry.lock().unwrap();
            guard.take().map(|f| f())
        })
        .collect();

    let timeout = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        futures::future::join_all(futures),
    )
    .await;

    if timeout.is_err() {
        eprintln!("[Warning] Cleanup functions timed out after 10s");
    }
}
