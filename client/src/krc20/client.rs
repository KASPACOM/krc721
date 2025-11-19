use crate::args::{Action, Args};
use krc721_core::model::kasplex::v1;
use krc721_core::runtime::Runtime;
use krc721_rs::krc20::v1::Indexer;
use krc721_rs::result::Result;

#[derive(Default)]
pub struct Client;

impl Client {
    pub async fn run(&self, _runtime: &Runtime) -> Result<()> {
        let Args {
            action,
            network_id,
            enable_debug_mode,
            trace_log_level,
        } = Args::parse();

        if trace_log_level {
            workflow_log::set_log_level(workflow_log::LevelFilter::Trace);
        }

        krc721_core::debug::enable(enable_debug_mode);

        // ---

        println!(
            "kasplex indexer client v{}-{} (rusty-kaspa v{}) - {network_id}",
            crate::VERSION,
            crate::GIT_DESCRIBE,
            kaspa_wallet_core::version()
        );

        let network = v1::Network::try_from(&network_id)?;

        match action {
            Action::List => {
                println!();

                let indexer = Indexer::try_new(network.into())?;
                let mut tokens = indexer.get_token_list().await?;
                tokens.sort_by(|a, b| a.tick.cmp(&b.tick));

                tokens.iter().for_each(|token| {
                    println!("{}", token.tick);
                });

                println!();
                println!("{} tokens", tokens.len());
                println!();
            }
            Action::Status => {
                let indexer = Indexer::try_new(network.into())?;
                let result = indexer.get_indexer_status().await?;
                println!();
                println!("{:>12}: {}", "Network", network_id);
                println!("{}", result.format(v1::Network::Testnet10));
                println!();
            }
        }

        Ok(())
    }
}
