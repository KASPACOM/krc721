use crate::args::{Action, Args, WalletAction};
use crate::wallet::*;
use cliclack::intro;
use console::style;
use krc721_core::model::kasplex;
use krc721_core::runtime::Runtime;
use krc721_rpc_client::prelude::*;
use krc721_rs::imports::*;
use krc721_rs::krc20::v1::Indexer as KasplexIndexer;
use krc721_rs::result::Result;
use workflow_log::prelude::*;

#[derive(Default)]
pub struct Client;

impl Client {
    pub async fn run(&self, _runtime: &Runtime) -> Result<()> {
        let Args {
            action,
            wallet_file,
            node_url,
            krc721d_url,
            network_id,
            enable_debug_mode,
            trace_log_level,
        } = Args::parse();

        if trace_log_level {
            workflow_log::set_log_level(workflow_log::LevelFilter::Trace);
        }

        krc721_core::debug::enable(enable_debug_mode);

        // ---

        let version = format!(
            "krc721 client v{}-{} (rusty-kaspa v{})",
            crate::VERSION,
            crate::GIT_DESCRIBE,
            kaspa_wallet_core::version()
        );

        let url = krc721d_url.unwrap_or_else(|| "ws://localhost:7878".to_string());

        match action {
            Action::Ping => {
                println!("{}", version);

                let client = Krc721RpcClient::try_new(url.as_str(), None)?;
                client.connect(None).await?;
                println!(
                    "Connected to {}",
                    client.url().unwrap_or_else(|| "🤷".to_string())
                );

                // ensure that krc721d network matches ours
                let _status = client.negotiate(&network_id).await?;

                println!("📡 Pinging...");
                client.ping().await?;
                println!("🥂 Ok...");
                client.disconnect().await?;
            }
            Action::Wallet { action } => {
                println!();
                crate::log::init();
                intro(style(version).on_black().cyan())?;

                let ctx = Context {
                    network_id,
                    node_url,
                    wallet_file,
                };

                match action {
                    WalletAction::NftDeploy => {
                        log_info!("Demo Deploy KRC-721");
                        // let wallet = Wallet::try_new(ctx, true).await?;

                        //  wallet.wallet.utxo_processor();
                        // log_info!("{:#?}", wallet.account);
                    }
                    WalletAction::NftMint => {
                        log_info!("Demo Mint KRC-721");
                        // let wallet = Wallet::try_new(ctx, true).await?;
                        // wallet.wallet.utxo_processor();
                        // log_info!("{:#?}", wallet.account);
                    }
                    WalletAction::NftTransfer => {
                        log_info!("Demo Transfer KRC-721");
                        // let wallet = Wallet::try_new(ctx, true).await?;
                        // wallet.wallet.utxo_processor();
                        // log_info!("{:#?}", wallet.account);
                    }
                    WalletAction::TokenDeploy => {
                        log_info!("Demo Deploy KRC-21");
                        // let wallet = Wallet::try_new(ctx, true).await?;
                        // wallet.wallet.utxo_processor();
                        // log_info!("{:#?}", wallet.account);
                    }
                    WalletAction::NftBalance => {
                        log_info!("Balance");
                        let _wallet = Wallet::try_new(ctx, true).await?;
                        // wallet.wallet.utxo_processor();
                        // log_info!("{:#?}", wallet.account);
                        // wallet.demo_deploy().await;
                        // wallet.demo_mint().await;

                        // some fake address (placeholder)
                        let address = match network_id.network_type() {
                            NetworkType::Testnet => Address::try_from("kaspatest:qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqhqrxplya").unwrap(),
                            NetworkType::Mainnet => Address::try_from("kaspa:qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqkx9awp4e").unwrap(),
                            _ => panic!("Unsupported network"),
                        };

                        let network = kasplex::v1::Network::try_from(&network_id)?;
                        let indexer = KasplexIndexer::try_new(network.into())?;
                        let mut tokens =
                            indexer.get_token_balance_list_by_address(&address).await?;
                        tokens.sort_by(|a, b| a.tick.cmp(&b.tick));
                    }
                }
            }
        }

        Ok(())
    }
}
