use hyper;
use futures;
use futures::{Future, Stream};
use errors::{Result, HueError};

pub use tokio_core::reactor::Core;

#[cfg(feature = "nupnp")]
use hyper_tls;

#[cfg(feature = "nupnp")]
/// a hyper HTTPS network client
pub type TlsClient = hyper::Client<hyper_tls::HttpsConnector<hyper::client::HttpConnector>>;

/// a hyper HTTP network client
pub type Client = hyper::Client<hyper::client::HttpConnector>;

/// an asynchronous IO result for our library
pub type HueFuture<'a, T> = Box<Future<Item = T, Error = HueError> + 'a>;

#[cfg(feature = "nupnp")]
/// create a reactor core and a hyper TLS client
pub fn make_core_and_tls_client() -> (Core, TlsClient) {
    let core = Core::new().unwrap();
    let client = hyper::Client::configure()
        .connector(hyper_tls::HttpsConnector::new(4, &core.handle()).unwrap())
        .build(&core.handle());
    (core, client)
}

/// extract the body from a `hyper::Response`
pub fn body_from_res(res: hyper::Response) -> futures::future::BoxFuture<String, HueError> {
    use std::str::from_utf8;
    res.body().concat2().from_err::<HueError>().and_then(|body| {
        let body: Result<String> = from_utf8(&body)
            .map_err(From::from)
            .map(String::from);
        futures::done(body)
    }).boxed()
}
