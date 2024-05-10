//! Safely make a non-[`Send`] future [`Send`]able.

use std::future::Future;

use futures::channel::oneshot;
use futures::FutureExt;

/// Wraps the future in [`LocalFuture`], taking ownership of captured variables if needed.
#[macro_export]
macro_rules! local_future {
    ($e:expr) => {
        $crate::local_future::LocalFuture::spawn(async move { { $e }.await })
    };
}

/// Safely makes non-[`Send`] future [`Send`]able by spawning it on the local executor.
pub struct LocalFuture<T>(oneshot::Receiver<T>);
impl<T> LocalFuture<T>
where
    T: 'static,
{
    /// Wraps the future.
    pub fn spawn(future: impl Future<Output = T> + 'static) -> Self {
        let (send, recv) = oneshot::channel();
        wasm_bindgen_futures::spawn_local(async move {
            let out = future.await;
            send.send(out).unwrap_or_else(|_| panic!());
        });
        Self(recv)
    }
}
impl<T> Future for LocalFuture<T> {
    type Output = T;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        self.0.poll_unpin(cx).map(Result::unwrap)
    }
}
