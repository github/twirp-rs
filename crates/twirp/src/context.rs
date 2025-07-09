use std::sync::{Arc, Mutex};

/// Context allows passing information between twirp rpc handlers and http middleware by providing
/// access to extensions on the `http::Request` and `http::Response`.
///
/// An example use case is to extract a request id from an http header and use that id in subsequent
/// handler code.
#[derive(Default)]
pub struct Context {
    req_extensions: http::Extensions,
    resp_extensions: Arc<Mutex<http::Extensions>>,
}

impl Context {
    pub fn new(
        req_extensions: http::Extensions,
        resp_extensions: Arc<Mutex<http::Extensions>>,
    ) -> Self {
        Self {
            req_extensions,
            resp_extensions,
        }
    }

    pub fn extensions_mut(&mut self) -> &mut http::Extensions {
        &mut self.req_extensions
    }

    /// Get a request extension.
    pub fn get<T>(&self) -> Option<&T>
    where
        T: Clone + Send + Sync + 'static,
    {
        self.req_extensions.get::<T>()
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
