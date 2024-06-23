use std::sync::Arc;
use std::time::Duration;

use async_broadcast::{broadcast, InactiveReceiver, RecvError, SendError, Sender};
use async_std::io::timeout;
use clap::Parser;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[arg(short, long)]
    quiet: bool,

    #[arg(short, long, action = clap::ArgAction::Count)]
    verbosity: u8,

    #[arg(short, long)]
    bind: Option<String>,
}

impl Cli {
    fn log_level(&self) -> tide::log::LevelFilter {
        use tide::log::LevelFilter;
        if self.quiet {
            LevelFilter::Error
        } else {
            match self.verbosity {
                0 => LevelFilter::Info,
                1 => LevelFilter::Debug,
                _ => LevelFilter::Trace,
            }
        }
    }

    fn bind_addr(&self) -> &str {
        self.bind.as_deref().unwrap_or("0.0.0.0:8090")
    }
}

#[derive(Clone)]
struct State {
    tx: Arc<Sender<Arc<Event>>>,
    rx: Arc<InactiveReceiver<Arc<Event>>>,
}

#[async_std::main]
async fn main() -> Result<(), std::io::Error> {
    let cli = Cli::parse();

    tide::log::with_level(cli.log_level());

    let (mut tx, rx) = broadcast(16);

    tx.set_overflow(true);
    tx.set_await_active(false);
    let tx = Arc::new(tx);
    let rx = Arc::new(rx.deactivate());

    let mut app = tide::with_state(State { tx, rx });

    app.at("/").post(post);
    app.at("/sse/").get(tide::sse::endpoint(handler));

    app.listen(cli.bind_addr()).await?;

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
