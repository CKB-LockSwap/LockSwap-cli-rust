use clap::{arg, Command};

pub fn cli() -> Command {
    Command::new("lockswap")
        .about("a simple swap for sUDT and CKB")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .allow_external_subcommands(true)
        .subcommand(Command::new("search_sudt").about("search on-chain sudt owned by private key"))
        .subcommand(
            Command::new("make_order")
                .about("make lockswap order on sudt")
                .arg(arg!(-s --sudt [SUDT] "insert sUDT cell offset in list"))
                .arg(arg!(-c --ckb [CKB] "insert order CKB number")),
        )
        .subcommand(
            Command::new("take_order")
                .about("take lockswap order on ckb")
                .arg(arg!(-l --lockswap [LOCKSWAP] "insert lockswap cell offset in list")),
        )
}
