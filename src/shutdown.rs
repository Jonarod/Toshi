use tokio::prelude::*;
use futures::future;

//#[cfg(unix)]
//pub fn shutdown() -> impl Future<Output = ()> + Send + 'static {
//    async {
//        use tokio::net::signal::{self, unix::{signal, SignalKind}};
//
//        let sigint = signal(SignalKind::int()?);
//        let sigterm = signal(SignalKind::term()?);
//
//        sigint.select(sigterm).into_future().await;
//        println!("got signal {:?}", signal);
//    }
//}

//#[cfg(not(unix))]
pub fn shutdown() -> impl Future<Output = ()> + Send + 'static {
    async {
        use tokio::net::signal::ctrl_c;

        let ctrlc = ctrl_c().unwrap();
        let prog = ctrlc.for_each(|_| {
            println!("ctrl-c received!");
            future::ready(())
        });

        prog.await;
    }
}