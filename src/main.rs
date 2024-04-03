use ckb_jsonrpc_types::OutputsValidator;
use ckb_sdk::core::TransactionBuilder;
use ckb_sdk::rpc::ckb_indexer::{Cell, Order, SearchMode};
use ckb_sdk::traits::DefaultCellDepResolver;
use ckb_sdk::transaction::builder::ChangeBuilder;
use ckb_sdk::transaction::input::{InputIterator, TransactionInput};
use ckb_sdk::transaction::signer::{SignContexts, TransactionSigner};
use ckb_sdk::transaction::{builder::DefaultChangeBuilder, TransactionBuilderConfiguration};
use ckb_sdk::types::transaction_with_groups::TransactionWithScriptGroupsBuilder;
use ckb_sdk::{traits::CellQueryOptions, CkbRpcClient, IndexerRpcClient};
use ckb_types::core::{Capacity, DepType, TransactionView};
use ckb_types::packed::{CellDep, CellInput, CellOutput, OutPoint};
use ckb_types::prelude::Unpack;
use ckb_types::{
    core::ScriptHashType,
    packed::Script,
    prelude::{Builder, Entity, Pack},
    H256,
};

mod cli;
mod config;

use cli::cli;
use config::{load_config, Config};

#[derive(serde::Serialize)]
struct SUDTOutput {
    number: u128,
    owner_lockhash: H256,
}

#[derive(serde::Serialize)]
struct LockSwapOutput {
    sudt: SUDTOutput,
    order_ckb: u64,
    maker_lockscript: ckb_jsonrpc_types::Script,
}

fn fetch_sudt_cells(config: &Config, indexer_rpc: &IndexerRpcClient) -> Vec<Cell> {
    let user_lock_script: Script = config.user_address.payload().into();
    let type_script = Script::new_builder()
        .code_hash(config.sudt_script.code_hash.pack())
        .hash_type(ScriptHashType::Type.into())
        .build();
    let mut search_cell = CellQueryOptions::new_lock(user_lock_script);
    search_cell.secondary_script = Some(type_script);
    search_cell.script_search_mode = Some(SearchMode::Prefix);
    let sudt_cells = indexer_rpc
        .get_cells(search_cell.into(), Order::Asc, 10.into(), None)
        .unwrap();
    sudt_cells.objects
}

fn fetch_lockswap_cells(config: &Config, indexer_rpc: &IndexerRpcClient) -> Vec<Cell> {
    let lock_script = Script::new_builder()
        .code_hash(config.lockswap_script.code_hash.pack())
        .hash_type(ScriptHashType::Type.into())
        .build();
    let mut search_cell = CellQueryOptions::new_lock(lock_script);
    search_cell.script_search_mode = Some(SearchMode::Prefix);
    let lockswap_cells = indexer_rpc
        .get_cells(search_cell.into(), Order::Asc, 10.into(), None)
        .unwrap();
    lockswap_cells.objects
}

fn complete_transaction_with_change(
    mut tx: TransactionBuilder,
    tx_config: &TransactionBuilderConfiguration,
    change_lock: Script,
    tx_inputs: Vec<TransactionInput>,
) -> TransactionView {
    let mut change_completer = DefaultChangeBuilder::new(tx_config, change_lock.clone(), tx_inputs);
    change_completer.init(&mut tx);
    let iterator = InputIterator::new(vec![change_lock.clone()], tx_config.network_info());
    let mut completed = false;
    for live_cell in iterator {
        let live_cell = live_cell.unwrap();
        tx.input(live_cell.cell_input());
        if change_completer.check_balance(live_cell, &mut tx) {
            completed = true;
            break;
        }
    }
    if !completed {
        panic!("insufficient capacity");
    }
    change_completer.finalize(tx)
}

fn add_secp256k1_sighash_celldep(tx: &mut TransactionBuilder, ckb_rpc: &CkbRpcClient) {
    let celldeps = DefaultCellDepResolver::from_genesis(
        &ckb_rpc
            .get_block_by_number(0.into())
            .unwrap()
            .unwrap()
            .into(),
    )
    .unwrap();
    let (sighash_celldep, _) = celldeps.sighash_dep().unwrap();
    tx.cell_dep(sighash_celldep.clone());
}

fn sign_transaction(
    tx: TransactionView,
    tx_config: &TransactionBuilderConfiguration,
    config: &Config,
    user_lock: &Script,
    indices: &[usize],
) -> TransactionView {
    let mut tx_groups = TransactionWithScriptGroupsBuilder::default()
        .set_tx_view(tx)
        .add_lock_script_group(user_lock, indices)
        .build();
    let signer = TransactionSigner::new(tx_config.network_info());
    let privkey = config.user_privkey.clone();
    signer
        .sign_transaction(&mut tx_groups, &SignContexts::new_sighash(vec![privkey]))
        .expect("sign");
    tx_groups.get_tx_view().clone()
}

fn main() {
    let config = load_config("./config.toml".parse().unwrap()).expect("config");
    let indexer_rpc = IndexerRpcClient::new(&config.ckb_url);
    let ckb_rpc = CkbRpcClient::new(&config.ckb_url);

    let matches = cli().get_matches();
    match matches.subcommand() {
        Some(("search_sudt", _)) => {
            let output_sudt_cells = fetch_sudt_cells(&config, &indexer_rpc)
                .into_iter()
                .map(|v| SUDTOutput {
                    number: u128::from_le_bytes(
                        v.output_data.unwrap().as_bytes().try_into().unwrap(),
                    ),
                    owner_lockhash: Script::from(v.output.type_.unwrap())
                        .calc_script_hash()
                        .unpack(),
                })
                .collect::<Vec<_>>();
            println!(
                "cells: {}",
                serde_json::to_string_pretty(&output_sudt_cells).unwrap()
            );
        }
        Some(("search_lockswap", _)) => {
            let output_lockswap_cells = fetch_lockswap_cells(&config, &indexer_rpc)
                .into_iter()
                .map(|v| LockSwapOutput {
                    sudt: SUDTOutput {
                        number: u128::from_le_bytes(
                            v.output_data.clone().unwrap().as_bytes()[0..16]
                                .try_into()
                                .unwrap(),
                        ),
                        owner_lockhash: Script::from(v.output.type_.unwrap())
                            .calc_script_hash()
                            .unpack(),
                    },
                    order_ckb: u64::from_le_bytes(
                        v.output_data.unwrap().as_bytes()[16..24]
                            .try_into()
                            .unwrap(),
                    ),
                    maker_lockscript: Script::from_compatible_slice(v.output.lock.args.as_bytes())
                        .unwrap()
                        .into(),
                })
                .collect::<Vec<_>>();
            println!(
                "cells: {}",
                serde_json::to_string_pretty(&output_lockswap_cells).unwrap()
            );
        }
        Some(("make_order", sub_matches)) => {
            let offset = sub_matches
                .get_one::<String>("sudt")
                .map(|v| usize::from_str_radix(v, 10).unwrap())
                .unwrap();
            let order_ckb = sub_matches
                .get_one::<String>("ckb")
                .map(|v| u64::from_str_radix(v, 10).unwrap())
                .unwrap();
            let Some(sudt_cell) = fetch_sudt_cells(&config, &indexer_rpc).get(offset).cloned()
            else {
                panic!("no sudt cell with offset {offset} exists");
            };
            let tx_input = TransactionInput::new(sudt_cell.clone().into(), 0);
            let user_lock: Script = config.user_address.payload().into();
            let lockswap_lock = Script::new_builder()
                .code_hash(config.lockswap_script.code_hash.pack())
                .hash_type(ScriptHashType::Type.into())
                .args(user_lock.as_bytes().pack())
                .build();
            let mut tx = TransactionBuilder::default();
            add_secp256k1_sighash_celldep(&mut tx, &ckb_rpc);
            tx.cell_dep(
                CellDep::new_builder()
                    .dep_type(DepType::Code.into())
                    .out_point(OutPoint::new(config.sudt_script.tx_hash.pack(), 0))
                    .build(),
            )
            .input(
                CellInput::new_builder()
                    .previous_output(sudt_cell.out_point.into())
                    .build(),
            )
            .output(
                CellOutput::new_builder()
                    .lock(lockswap_lock)
                    .type_(sudt_cell.output.type_.map(Into::into).pack())
                    .build_exact_capacity(Capacity::bytes(24).unwrap())
                    .unwrap(),
            )
            .output_data(
                vec![
                    sudt_cell.output_data.unwrap().as_bytes().to_vec(),
                    (order_ckb * 100_000_000).to_le_bytes().to_vec(),
                ]
                .concat()
                .pack(),
            );
            let mut tx_config = TransactionBuilderConfiguration::new_testnet().unwrap();
            tx_config.fee_rate = 10000;
            let tx =
                complete_transaction_with_change(tx, &tx_config, user_lock.clone(), vec![tx_input]);
            let sign_indices = (0..tx.inputs().len()).collect::<Vec<_>>();
            let signed_tx = sign_transaction(tx, &tx_config, &config, &user_lock, &sign_indices);
            let tx_hash = ckb_rpc
                .send_transaction(signed_tx.data().into(), Some(OutputsValidator::Passthrough))
                .expect("send tx");
            println!("tx_hash = {}", hex::encode(&tx_hash));
        }
        Some(("take_order", sub_matches)) => {
            let offset = sub_matches
                .get_one::<String>("lockswap")
                .map(|v| usize::from_str_radix(v, 10).unwrap())
                .unwrap();
            let Some(lockswap_cell) = fetch_lockswap_cells(&config, &indexer_rpc)
                .get(offset)
                .cloned()
            else {
                panic!("no lockswap cell with offset {offset} exists");
            };
            let tx_input = TransactionInput::new(lockswap_cell.clone().into(), 0);
            let maker_lockscript =
                Script::from_compatible_slice(lockswap_cell.output.lock.args.as_bytes()).unwrap();
            let maker_order_ckb = u64::from_le_bytes(
                lockswap_cell.output_data.clone().unwrap().as_bytes()[16..24]
                    .try_into()
                    .unwrap(),
            );
            let maker_sudt = u128::from_le_bytes(
                lockswap_cell.output_data.unwrap().as_bytes()[0..16]
                    .try_into()
                    .unwrap(),
            );
            println!(
                "deal: {} CKB => {} SUDT",
                Capacity::shannons(maker_order_ckb),
                maker_sudt
            );
            let user_lock: Script = config.user_address.payload().into();
            let mut tx = TransactionBuilder::default();
            add_secp256k1_sighash_celldep(&mut tx, &ckb_rpc);
            tx.cell_dep(
                CellDep::new_builder()
                    .dep_type(DepType::Code.into())
                    .out_point(OutPoint::new(config.sudt_script.tx_hash.pack(), 0))
                    .build(),
            )
            .cell_dep(
                CellDep::new_builder()
                    .dep_type(DepType::Code.into())
                    .out_point(OutPoint::new(config.lockswap_script.tx_hash.pack(), 0))
                    .build(),
            )
            .input(
                CellInput::new_builder()
                    .previous_output(lockswap_cell.out_point.into())
                    .build(),
            )
            .output(
                CellOutput::new_builder()
                    .lock(user_lock.clone())
                    .type_(lockswap_cell.output.type_.map(Into::into).pack())
                    .build_exact_capacity(Capacity::bytes(16).unwrap())
                    .unwrap(),
            )
            .output_data(maker_sudt.to_le_bytes().to_vec().pack())
            .output(
                CellOutput::new_builder()
                    .lock(maker_lockscript)
                    .capacity(Capacity::shannons(maker_order_ckb).pack())
                    .build(),
            )
            .output_data(Default::default());
            let mut tx_config = TransactionBuilderConfiguration::new_testnet().unwrap();
            tx_config.fee_rate = 10000;
            let tx =
                complete_transaction_with_change(tx, &tx_config, user_lock.clone(), vec![tx_input]);
            let sign_indices = (1..tx.inputs().len()).collect::<Vec<_>>();
            let signed_tx = sign_transaction(tx, &tx_config, &config, &user_lock, &sign_indices);
            let tx_hash = ckb_rpc
                .send_transaction(signed_tx.data().into(), Some(OutputsValidator::Passthrough))
                .expect("send tx");
            println!("tx_hash = {}", hex::encode(&tx_hash));
        }
        _ => unreachable!(),
    }
}
