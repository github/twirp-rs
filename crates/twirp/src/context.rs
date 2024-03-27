use std::sync::{Arc, Mutex};

use http::Extensions;

/// Context allows passing information between twirp rpc handlers and http middleware by providing
/// access to extensions on the `http:Request` and `http:Response`.
///
/// An example use case is to extract a request id from an http header and use that id in subsequent
/// handler code.
#[derive(Default)]
pub struct Context {
    extensions: Extensions,
    resp_extensions: Arc<Mutex<Extensions>>,
}

impl Context {
    pub fn new(extensions: Extensions, resp_extensions: Arc<Mutex<Extensions>>) -> Self {
        Self {
            extensions,
            resp_extensions,
        }
    }

    /// Get a request extension.
    pub fn get<T>(&self) -> Option<&T>
    where
        T: Clone + Send + Sync + 'static,
    {
        self.extensions.get::<T>()
    }

    /// Insert a response extension.
    pub fn insert<T>(&self, val: T) -> Option<T>
    where
        T: Clone + Send + Sync + 'static,
    {
        self.resp_extensions
            .lock()
            .expect("mutex poisoned")
            .insert(val)
    }
}
