//! `quittance`: prove one shielded Zcash payment, bound to a verifier's
//! challenge, without handing over a viewing key.

mod display;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use quittance_wallet::{
    parse_transaction, parse_txid, DisclosureRequest, IronwoodRecipient, Lightwalletd, PaymentRequest, SyncEvent, TxId,
    Wallet,
};
use rand::{rand_core::UnwrapErr, rngs::SysRng, Rng};
use zip311::{Challenge, Network, OutputSelection, PaymentDisclosure, VerifyContext};

#[derive(Parser)]
#[command(name = "quittance", version, about = "Prove one shielded Zcash payment, bound to a verifier's challenge, without handing over a viewing key")]
struct Cli {
    /// lightwalletd server(s) to use instead of the public ones, e.g. https://testnet.zec.rocks:443
    #[arg(long, global = true, env = "QUITTANCE_LIGHTWALLETD", value_delimiter = ',')]
    server: Vec<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// The Ironwood-only wallet that sends payments and keeps their disclosure kits
    #[command(subcommand)]
    Wallet(WalletCommand),
    /// Disclosure kits: the spent notes and Merkle paths a disclosure re-proves
    #[command(subcommand)]
    Kit(KitCommand),
    /// Answer a verifier's challenge about a payment this wallet sent
    Prove(ProveArgs),
    /// Check a payment disclosure against the chain
    Verify(VerifyArgs),
    /// Challenges for a verifier to hand to the payer
    #[command(subcommand)]
    Challenge(ChallengeCommand),
    /// A transaction as the chain has it: mined height and raw bytes
    Tx {
        txid: String,
        #[arg(long, value_enum, default_value = "test")]
        network: NetworkArg,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Args)]
struct DirArg {
    /// The wallet directory
    #[arg(long)]
    dir: PathBuf,
}

#[derive(Subcommand)]
enum WalletCommand {
    /// Create a wallet for account 0 of a mnemonic
    Init {
        #[arg(long, value_enum)]
        network: NetworkArg,
        #[arg(long)]
        dir: PathBuf,
        /// A file with a line `mnemonic: <24 words>`; it stays where it is and is read whenever a key is needed
        #[arg(long)]
        mnemonic_file: PathBuf,
        /// The first block that can hold the wallet's funds
        #[arg(long)]
        birthday: u32,
    },
    /// Scan the chain up to the tip (repeatable; picks up where it left off)
    Sync(DirArg),
    /// Balances by pool
    Balance {
        #[command(flatten)]
        dir: DirArg,
        #[arg(long)]
        json: bool,
    },
    /// The wallet's address (Orchard receiver only: payments to it land in Ironwood)
    Address(DirArg),
    /// Pay an Ironwood address and capture the payment's disclosure kit
    Send {
        #[command(flatten)]
        dir: DirArg,
        /// A unified address with an Orchard receiver
        #[arg(long)]
        to: String,
        /// The amount in zatoshis (1 ZEC = 100,000,000)
        #[arg(long)]
        zat: u64,
        #[arg(long)]
        memo: Option<String>,
    },
    /// Recent transactions
    List {
        #[command(flatten)]
        dir: DirArg,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum KitCommand {
    /// Capture (or recapture) the kit for a transaction this wallet sent
    Capture {
        #[command(flatten)]
        dir: DirArg,
        #[arg(long)]
        txid: String,
    },
}

#[derive(Args)]
struct ProveArgs {
    #[command(flatten)]
    dir: DirArg,
    #[arg(long)]
    txid: String,
    /// The verifier's challenge, or @file to read it from a file
    #[arg(long)]
    challenge: String,
    /// `payments` (every output to someone else) or action indices like `0,2`
    #[arg(long, default_value = "payments")]
    outputs: String,
    /// Include the raw transaction so the verifier needn't look it up by txid
    #[arg(long)]
    embed_tx: bool,
    /// Write the disclosure here instead of printing it
    #[arg(long)]
    out: Option<PathBuf>,
}

#[derive(Args)]
struct VerifyArgs {
    /// The disclosure (zpd1… / zpdtest1…), or @file
    disclosure: String,
    /// The challenge you issued, or @file; without it the disclosure could be a replay
    #[arg(long)]
    challenge: Option<String>,
    /// The network you expect; defaults to the one the disclosure names
    #[arg(long, value_enum)]
    network: Option<NetworkArg>,
    #[arg(long)]
    json: bool,
}

#[derive(Subcommand)]
enum ChallengeCommand {
    /// A fresh challenge: who asks, about what, a random nonce and an expiry
    New {
        #[arg(long)]
        verifier: String,
        #[arg(long)]
        context: String,
        /// Seconds until it expires
        #[arg(long, default_value_t = 3600)]
        ttl: u64,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum NetworkArg {
    Test,
    Main,
}

impl From<NetworkArg> for Network {
    fn from(n: NetworkArg) -> Self {
        match n {
            NetworkArg::Test => Network::Test,
            NetworkArg::Main => Network::Main,
        }
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli).await {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(2)
        }
    }
}

async fn run(cli: Cli) -> Result<ExitCode> {
    let servers = cli.server;
    match cli.command {
        Command::Wallet(c) => wallet(c, &servers).await?,
        Command::Kit(KitCommand::Capture { dir, txid }) => {
            let txid = txid_arg(&txid)?;
            let mut wallet = Wallet::open(&dir.dir)?;
            let mut lwd = connect(wallet.network(), &servers).await?;
            let kit = wallet.capture_kit(&mut lwd, txid).await?;
            println!("kit for {txid}: {} spent note(s), anchor at height {}", kit.notes.len(), kit.anchor_height);
            println!("saved {}", wallet.kit_path(txid).display());
        }
        Command::Prove(args) => prove(args, &servers).await?,
        Command::Verify(args) => return verify(args, &servers).await,
        Command::Tx { txid, network, json } => {
            let txid = txid_arg(&txid)?;
            let network = Network::from(network);
            let mut lwd = connect(network, &servers).await?;
            let chain = lwd.transaction(txid).await?.ok_or_else(|| anyhow!("transaction {txid} is not on {network}"))?;
            if json {
                let value = serde_json::json!({
                    "network": network,
                    "txid": txid.to_string(),
                    "mined_height": chain.mined_height,
                    "raw": hex::encode(&chain.raw),
                });
                println!("{}", serde_json::to_string_pretty(&value)?);
            } else {
                match chain.mined_height {
                    Some(h) => println!("mined at {h}"),
                    None => println!("not mined"),
                }
                println!("{}", hex::encode(&chain.raw));
            }
        }
        Command::Challenge(ChallengeCommand::New { verifier, context, ttl }) => {
            let mut nonce = [0u8; 16];
            UnwrapErr(SysRng).fill_bytes(&mut nonce);
            let c = Challenge::new(&verifier, &context, nonce, unix_now() + ttl).map_err(|e| anyhow!(e))?;
            println!("{}", String::from_utf8(c.to_msg()).expect("challenges are text"));
        }
    }
    Ok(ExitCode::SUCCESS)
}

async fn wallet(c: WalletCommand, servers: &[String]) -> Result<()> {
    match c {
        WalletCommand::Init { network, dir, mnemonic_file, birthday } => {
            let network = Network::from(network);
            let mut lwd = connect(network, servers).await?;
            let wallet = Wallet::init(&dir, &mnemonic_file, birthday, &mut lwd).await?;
            println!("created a {network} wallet in {}", dir.display());
            println!("birthday  {birthday}");
            println!("address   {}", wallet.address()?);
            println!("next: quittance wallet sync --dir {}", quoted(&dir));
        }
        WalletCommand::Sync(DirArg { dir }) => {
            let mut wallet = Wallet::open(&dir)?;
            let mut lwd = connect(wallet.network(), servers).await?;
            let server = lwd.url().to_string();
            let report = wallet
                .sync(&mut lwd, |e| match e {
                    SyncEvent::Tip(h) => println!("chain tip {h} ({server})"),
                    SyncEvent::Scanned { blocks, remaining } => {
                        println!("scanned {}..{}  {remaining} blocks to go", blocks.start, blocks.end - 1)
                    }
                    SyncEvent::Rewound { fork_at, to } => println!("reorg at {fork_at}: rewound to {to}"),
                    SyncEvent::Enhanced(txid) => println!("fetched {txid}"),
                })
                .await?;
            println!("synced to {} ({} blocks scanned, {} transactions fetched)", report.tip, report.scanned_blocks, report.enhanced);
        }
        WalletCommand::Balance { dir, json } => {
            let wallet = Wallet::open(&dir.dir)?;
            let report = wallet.balance()?.ok_or(quittance_wallet::Error::NotSynced)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                display::balance(&report);
            }
        }
        WalletCommand::Address(DirArg { dir }) => {
            println!("{}", Wallet::open(&dir)?.address()?);
        }
        WalletCommand::Send { dir, to, zat, memo } => {
            let mut wallet = Wallet::open(&dir.dir)?;
            let network = wallet.network();
            // Refuse a non-Ironwood recipient before touching the network.
            IronwoodRecipient::parse(&to, network)?;
            let mut lwd = connect(network, servers).await?;
            // Spend from the chain as it is now, not as it was at the last sync.
            wallet.sync(&mut lwd, |_| {}).await?;
            let sent = wallet.send(&mut lwd, &PaymentRequest { to, zat, memo }).await?;
            let ticker = display::ticker(network);
            println!("paid    {} {ticker} to {}", display::zec(zat), sent.paid_to);
            println!("fee     {} {ticker}", display::zec(sent.fee_zat));
            println!("txid    {}", sent.txid);
            match sent.kit {
                Ok(path) => println!("kit     {}", path.display()),
                Err(e) => {
                    eprintln!("warning: the disclosure kit was not captured: {e}");
                    eprintln!("  once the transaction is mined, run: quittance kit capture --dir {} --txid {}", quoted(&dir.dir), sent.txid);
                }
            }
        }
        WalletCommand::List { dir, limit, json } => {
            let wallet = Wallet::open(&dir.dir)?;
            let txs = wallet.history(limit)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&txs)?);
            } else {
                display::history(&txs, wallet.network());
            }
        }
    }
    Ok(())
}

async fn prove(args: ProveArgs, servers: &[String]) -> Result<()> {
    let txid = txid_arg(&args.txid)?;
    let msg = bytes_arg(&args.challenge)?;
    let outputs = outputs_arg(&args.outputs)?;
    if let Some(c) = Challenge::parse(&msg) {
        if c.is_expired(unix_now()) {
            eprintln!("warning: this challenge expired at unix time {}; the verifier will see that", c.expires);
        }
    }
    let wallet = Wallet::open(&args.dir.dir)?;
    // Fail on a missing or damaged kit before the slow part.
    wallet.load_kit(txid)?.witnesses()?;
    let mut lwd = connect(wallet.network(), servers).await?;
    eprintln!("building the proving key...");
    let pk = zip311::proving_key();
    eprintln!("proving...");
    let d = wallet.prove(&mut lwd, DisclosureRequest { txid, msg, outputs, embed_tx: args.embed_tx }, &pk).await?;
    let text = d.encode();
    match args.out {
        Some(path) => {
            std::fs::write(&path, format!("{text}\n")).with_context(|| format!("writing {}", path.display()))?;
            eprintln!("wrote {} ({} characters)", path.display(), text.len());
        }
        None => println!("{text}"),
    }
    Ok(())
}

async fn verify(args: VerifyArgs, servers: &[String]) -> Result<ExitCode> {
    let text = String::from_utf8(bytes_arg(&args.disclosure)?).context("the disclosure is not text")?;
    let d = PaymentDisclosure::decode(&text)?;
    let network = args.network.map(Network::from).unwrap_or(d.network);
    let expected = args.challenge.as_deref().map(bytes_arg).transpose()?;

    let mut lwd = connect(network, servers).await?;
    let txid = TxId::from_bytes(d.txid);
    let chain = lwd.transaction(txid).await?;
    let (tx, mined) = match (&d.tx, chain) {
        // An embedded transaction saves the lookup by txid, but whether (and
        // where) it was mined still comes from the chain.
        (Some(raw), chain) => (
            parse_transaction(raw, network, d.height).context("the embedded transaction does not parse")?,
            chain.and_then(|c| c.mined_height),
        ),
        (None, Some(c)) => (c.tx, c.mined_height),
        (None, None) => bail!("transaction {txid} is not on {network}, so there is nothing to check this disclosure against"),
    };

    let vk = zip311::verifying_key();
    let cx = VerifyContext { network, expected_msg: expected.as_deref(), vk: &vk, now: unix_now() };
    let verdict = zip311::verify(&d, &tx, mined, &cx);
    if args.json {
        let mut value = serde_json::to_value(&verdict)?;
        value["not_revealed"] = serde_json::json!(zip311::NOT_REVEALED);
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        display::verdict(&verdict);
    }
    Ok(if verdict.valid { ExitCode::SUCCESS } else { ExitCode::from(1) })
}

async fn connect(network: Network, servers: &[String]) -> Result<Lightwalletd> {
    Ok(Lightwalletd::connect(network, servers).await?)
}

fn txid_arg(s: &str) -> Result<TxId> {
    parse_txid(s).map_err(|e| anyhow!(e))
}

/// The argument itself, or the contents of the file after `@`. A single
/// trailing newline is dropped, so `echo msg > file` and `msg` sign the same bytes.
fn bytes_arg(arg: &str) -> Result<Vec<u8>> {
    let Some(path) = arg.strip_prefix('@') else {
        return Ok(arg.as_bytes().to_vec());
    };
    let mut bytes = std::fs::read(path).with_context(|| format!("reading {path}"))?;
    if bytes.ends_with(b"\n") {
        bytes.pop();
        if bytes.ends_with(b"\r") {
            bytes.pop();
        }
    }
    Ok(bytes)
}

fn outputs_arg(s: &str) -> Result<OutputSelection> {
    if s.trim().eq_ignore_ascii_case("payments") {
        return Ok(OutputSelection::Payments);
    }
    let indices = s
        .split(',')
        .map(|i| i.trim().parse::<u32>().map_err(|_| anyhow!("--outputs takes `payments` or action indices like `0,2`, not `{s}`")))
        .collect::<Result<Vec<_>>>()?;
    Ok(OutputSelection::Indices(indices))
}

fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn quoted(p: &Path) -> String {
    let s = p.display().to_string();
    if s.contains(' ') {
        format!("\"{s}\"")
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn challenge_files_lose_one_trailing_newline() {
        let dir = std::env::temp_dir().join(format!("quittance-cli-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("c.txt");
        std::fs::write(&path, "quittance:v1|a|b\r\n").unwrap();
        assert_eq!(bytes_arg(&format!("@{}", path.display())).unwrap(), b"quittance:v1|a|b");
        std::fs::write(&path, "two\n\n").unwrap();
        assert_eq!(bytes_arg(&format!("@{}", path.display())).unwrap(), b"two\n");
        assert_eq!(bytes_arg("as typed\n").unwrap(), b"as typed\n");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn output_selection() {
        assert!(matches!(outputs_arg("payments").unwrap(), OutputSelection::Payments));
        assert!(matches!(outputs_arg("0, 2").unwrap(), OutputSelection::Indices(v) if v == [0, 2]));
        assert!(outputs_arg("first").is_err());
    }
}
