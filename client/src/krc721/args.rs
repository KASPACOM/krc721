use kaspa_consensus_core::network::{NetworkId, NetworkType};

#[derive(Debug)]
pub enum BetaAction {
    Omega,
    Kappa,
}

#[derive(Debug)]
pub struct Args {
    pub trace_log_level: bool,
    pub enable_debug_mode: bool,
    pub node_url: Option<String>,
    pub krc721d_url: Option<String>,
    pub network_id: NetworkId,
    pub wallet_file: Option<String>,
    pub action: Action,
}

#[derive(Debug)]
pub enum Action {
    Ping,
    Wallet { action: WalletAction },
}

#[derive(Debug)]
pub enum WalletAction {
    NftBalance,
    NftDeploy,
    NftMint,
    NftTransfer,
    TokenDeploy,
}

impl Args {
    pub fn parse() -> Args {
        #[allow(unused)]
        use clap::{arg, command, Arg, Command};

        let cmd = Command::new("krc721")
            .about(format!(
                "krc721 client v{}-{} (rusty-kaspa v{})",
                crate::VERSION,
                crate::GIT_DESCRIBE,
                kaspa_wallet_core::version()
            ))
            .arg(arg!(--version "Display software version"))
            .arg(arg!(--trace "Enable trace log level"))
            .arg(arg!(--debug "Enable debug mode"))
            .arg(
                Arg::new("network")
                    .long("network")
                    .value_name("mainnet | testnet-10")
                    .num_args(0..=1)
                    .require_equals(true)
                    .value_parser(clap::value_parser!(NetworkId))
                    .help("Network id (default 'testnet-11')"),
            )
            .arg(
                Arg::new("rpc")
                    .long("rpc")
                    .value_name("ws://address[:port] or wss://address[:port]")
                    .num_args(0..=1)
                    .require_equals(true)
                    .help("wRPC URL of the krc721d daemon"),
            )
            .arg(
                Arg::new("node-rpc")
                    .long("node-rpc")
                    .value_name("ws://address[:port] or wss://address[:port]")
                    .num_args(0..=1)
                    .require_equals(true)
                    .help("wRPC URL of the rusty kaspa node"),
            )
            .subcommand(Command::new("ping").about("Ping krc721d daemon"))
            .subcommand(
                Command::new("wallet")
                    .about("Perform wallet operation")
                    // .subcommand(Command::new("list").about("List wallets"))
                    .subcommand(
                        Command::new("nftbalance")
                            .about("List NFTs")
                            .arg(Arg::new("wallet-file").help("Wallet file name")),
                    )
                    .subcommand(
                        Command::new("nftdeploy")
                            .about("Deploy demo NFT")
                            .arg(Arg::new("wallet-file").help("Wallet file name")),
                    )
                    .subcommand(
                        Command::new("nftmint")
                            .about("Mint demo NFT")
                            .arg(Arg::new("wallet-file").help("Wallet file name")),
                    )
                    .subcommand(
                        Command::new("nfttransfer")
                            .about("Transfer demo NFT")
                            .arg(Arg::new("wallet-file").help("Wallet file name")),
                    )
                    .subcommand(
                        Command::new("tokendeploy")
                            .about("KRC-21 token demo deploy")
                            .arg(Arg::new("wallet-file").help("Wallet file name")),
                    ),
                // .subcommand(
                //     // Command::new("accounts")
                //     Command::new("test").about("List wallet accounts").arg(
                //         Arg::new("wallet-file")
                //             .required(true)
                //             .help("Wallet file name"),
                //     ),
                // ), // .subcommand(Command::new("balance").about("List wallet balance"))
            );

        let matches = cmd.get_matches();

        let trace_log_level = matches.get_one::<bool>("trace").cloned().unwrap_or(false);

        let enable_debug_mode = matches.get_one::<bool>("debug").cloned().unwrap_or(false);

        let network_id = matches
            .get_one::<NetworkId>("network")
            .cloned()
            .unwrap_or(NetworkId::with_suffix(NetworkType::Testnet, 10));

        let node_url = matches.get_one::<String>("node-rpc").cloned();
        let krc721d_url = matches.get_one::<String>("rpc").cloned();
        let mut wallet_file = None;

        let action = if matches.get_one::<bool>("version").cloned().unwrap_or(false) {
            println!("v{}-{}", crate::VERSION, crate::GIT_DESCRIBE);
            std::process::exit(0);
        } else if let Some(_matches) = matches.subcommand_matches("ping") {
            Action::Ping
        } else if let Some(matches) = matches.subcommand_matches("wallet") {
            if let Some(matches) = matches.subcommand_matches("balance") {
                wallet_file = matches.get_one::<String>("wallet-file").cloned();
                Action::Wallet {
                    action: WalletAction::NftBalance,
                }
            } else if let Some(matches) = matches.subcommand_matches("nftdeploy") {
                wallet_file = matches.get_one::<String>("wallet-file").cloned();
                Action::Wallet {
                    action: WalletAction::NftDeploy,
                }
            } else if let Some(matches) = matches.subcommand_matches("nftmint") {
                wallet_file = matches.get_one::<String>("wallet-file").cloned();
                Action::Wallet {
                    action: WalletAction::NftMint,
                }
            } else if let Some(matches) = matches.subcommand_matches("nfttransfer") {
                wallet_file = matches.get_one::<String>("wallet-file").cloned();
                Action::Wallet {
                    action: WalletAction::NftTransfer,
                }
            } else if let Some(matches) = matches.subcommand_matches("tokendeploy") {
                wallet_file = matches.get_one::<String>("wallet-file").cloned();
                Action::Wallet {
                    action: WalletAction::TokenDeploy,
                }
            } else {
                println!("No wallet action specified");
                std::process::exit(1);
            }
        } else {
            println!("No command specified");
            std::process::exit(1);
        };

        Args {
            trace_log_level,
            enable_debug_mode,
            node_url,
            krc721d_url,
            network_id,
            wallet_file,
            action,
        }
    }
}
