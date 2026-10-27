//! Human-readable output.

use quittance_wallet::{BalanceReport, PoolBalance, TxSummary};
use zip311::{CheckStatus, MemoView, Network, Verdict, NOT_REVEALED};

pub fn ticker(network: Network) -> &'static str {
    match network {
        Network::Main => "ZEC",
        Network::Test => "TAZ",
    }
}

pub fn zec(zat: u64) -> String {
    let s = format!("{}.{:08}", zat / 100_000_000, zat % 100_000_000);
    let s = s.trim_end_matches('0');
    if s.ends_with('.') {
        format!("{s}0")
    } else {
        s.to_string()
    }
}

fn signed_zec(zat: i64) -> String {
    let sign = if zat < 0 { "-" } else { "+" };
    format!("{sign}{}", zec(zat.unsigned_abs()))
}

pub fn balance(r: &BalanceReport) {
    let t = ticker(r.network);
    println!("{} · scanned to {} (chain tip {})", r.network, r.scanned_to, r.chain_tip);
    let PoolBalance { spendable, pending, total } = r.ironwood;
    println!("Ironwood  {} {t} spendable, {} {t} pending, {} {t} total", zec(spendable), zec(pending), zec(total));
    for (name, b) in [("Orchard ", r.orchard), ("Sapling ", r.sapling)] {
        if b.total > 0 {
            println!("{name}  {} {t} (never spent by this wallet)", zec(b.total));
        }
    }
}

pub fn history(txs: &[TxSummary], network: Network) {
    if txs.is_empty() {
        println!("no transactions yet");
        return;
    }
    let t = ticker(network);
    for tx in txs {
        let height = match (tx.mined_height, tx.expired) {
            (Some(h), _) => h.to_string(),
            (None, true) => "expired".into(),
            (None, false) => "pending".into(),
        };
        let amount = format!("{} {t}", signed_zec(tx.net_zat));
        let kit = if tx.kit { "kit" } else { "   " };
        let memo = tx.memo.as_deref().map(|m| format!("\"{}\"", clip(m, 48))).unwrap_or_default();
        println!("{}  {height:>8}  {amount:>16}  {kit}  {memo}", tx.txid);
    }
}

pub fn verdict(v: &Verdict) {
    let t = ticker(v.network);
    println!("Payment disclosure · {} · tx {} · height {}", v.network, v.txid, v.height);
    println!();
    let width = v.checks.iter().map(|c| c.label.chars().count()).max().unwrap_or(0);
    for c in &v.checks {
        let mark = match c.status {
            CheckStatus::Pass => "✓",
            CheckStatus::Fail => "✗",
            CheckStatus::Warn => "!",
            CheckStatus::Skip => "-",
        };
        println!("  {mark} {:<width$}  {}", c.label, c.detail);
    }
    println!();
    match &v.revealed {
        Some(r) if v.valid => {
            println!("VALID: the sender of this transaction proved it paid");
            for p in &r.payments {
                println!("  {} {t}  ({} zat)  to {}", p.value_zec, p.value_zat, p.recipient);
                match &p.memo {
                    MemoView::Empty => println!("  no memo"),
                    MemoView::Text(s) => println!("  memo \"{s}\""),
                    MemoView::Bytes(h) => println!("  memo (binary) {h}"),
                }
            }
            if r.payments.is_empty() {
                println!("  (no payment opened: this shows who spent, not whom they paid)");
            }
        }
        _ => {
            let reason = v.checks.iter().find(|c| c.status == CheckStatus::Fail).map(|c| c.detail.as_str()).unwrap_or("it proves nothing");
            println!("NOT VALID: {reason}");
        }
    }
    if let Some(text) = &v.challenge_text {
        println!();
        println!("signed challenge: {text}");
    }
    println!();
    println!("never revealed:");
    for item in NOT_REVEALED {
        println!("  · {item}");
    }
}

fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max - 1).collect::<String>() + "…"
    }
}
