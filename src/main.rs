use std::sync::Arc;
use std::time::Duration;

use async_broadcast::{broadcast, InactiveReceiver, RecvError, SendError, Sender};
use async_std::io::timeout;

#[derive(Clone)]
struct State {
    tx: Arc<Sender<Arc<Event>>>,
    rx: Arc<InactiveReceiver<Arc<Event>>>,
}

#[async_std::main]
async fn main() -> Result<(), std::io::Error> {
    tide::log::start();

    let (mut tx, rx) = broadcast(16);

    tx.set_overflow(true);
    tx.set_await_active(false);
    let tx = Arc::new(tx);
    let rx = Arc::new(rx.deactivate());

    let mut app = tide::with_state(State { tx, rx });

    app.at("/").post(post);
    app.at("/sse/").get(tide::sse::endpoint(handler));

    app.listen("0.0.0.0:8090").await?;

    Ok(())
}

#[derive(Debug, serde::Deserialize)]
struct Event {
    name: String,
    data: String,
}

async fn handler(req: tide::Request<State>, sender: tide::sse::Sender) -> tide::Result<()> {
    let dur = Duration::from_secs(30);
    timeout(dur, sender.send("server", "hello", None)).await?;
    let mut rx = req.state().rx.activate_cloned();
    loop {
        match rx.recv_direct().await {
            Ok(ev) => timeout(dur, sender.send(&ev.name, &ev.data, None)).await?,
            Err(RecvError::Overflowed(n)) => {
                tide::log::debug!("Overflowed messages: {}", n);
                continue;
            }
            Err(e @ RecvError::Closed) => {
                tide::log::error!("Channel closed");
                return Err(e.into());
            }
        }
    }
}

async fn post(mut req: tide::Request<State>) -> tide::Result<tide::StatusCode> {
    let event: Event = req.body_json().await?;
    tide::log::debug!("{:?}", event);
    let event = Arc::new(event);
    // Ignore SendErrors, since there might not be any receivers
    let (Ok(_) | Err(SendError(_))) = req.state().tx.broadcast_direct(event).await;
    Ok(tide::StatusCode::Ok)
}
