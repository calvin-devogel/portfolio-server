use rustls::crypto::CryptoProvider;
use std::fmt::{Debug, Display};
use tokio::task::JoinError;

use portfolio_server::{
    configuration::get_configuration,
    startup::Application,
    telemetry::{get_subscriber, init_subscriber},
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // kick off the crypto provider in advance of authentication implementation
    // seems like maybe alpine doesn't specify a default provider at the OS level?
    // this might not be what's actually happening, but this does make auth work inside the container.
    let _ = CryptoProvider::install_default(rustls::crypto::aws_lc_rs::default_provider());

    // start logging (or console?)
    if std::env::var("TOKIO_CONSOLE").is_ok() {
        console_subscriber::init();
    } else {
        let subscriber = get_subscriber("portfolio_server".into(), "info".into(), std::io::stdout);
        init_subscriber(subscriber);
    }

    let configuration = get_configuration().expect("Failed to read configuration.");
    let application = Application::build(configuration.clone()).await?;
    let application_task = tokio::spawn(application.run_until_stopped());

    // put a tokio-select in here when you're ready
    tokio::select! {
        o = application_task => report_exit("API", o)
    }

    Ok(())
}

// return when the provided task exits (ie. when a background delivery worker finishes)
fn report_exit(task_name: &str, outcome: Result<Result<(), impl Debug + Display>, JoinError>) {
    match outcome {
        Ok(Ok(())) => {
            tracing::info!("{} has exited", task_name)
        }
        Ok(Err(e)) => {
            tracing::error!(
                error.cause_chain = ?e,
                error.message = %e,
                "{} failed",
                task_name
            )
        }
        Err(e) => {
            tracing::error!(
                error.cause_chain = ?e,
                error.message = %e,
                "{}' task failed to complete",
                task_name
            )
        }
    }
}
