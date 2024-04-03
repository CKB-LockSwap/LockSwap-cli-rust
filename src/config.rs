use std::fs;
use std::path::PathBuf;
use std::str::FromStr;

use ckb_sdk::Address;
use ckb_types::H256;
use secp256k1::SecretKey;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct ContractInfo {
    pub code_hash: H256,
    pub tx_hash: H256,
}

#[derive(Deserialize)]
pub struct Config {
    pub ckb_url: String,
    pub sudt_script: ContractInfo,
    pub lockswap_script: ContractInfo,
    #[serde(deserialize_with = "user_privkey_deser")]
    pub user_privkey: SecretKey,
    #[serde(deserialize_with = "user_address_deser")]
    pub user_address: Address,
}

fn user_privkey_deser<'de, D>(deserializer: D) -> Result<SecretKey, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let privkey: String = serde::Deserialize::deserialize(deserializer)?;
    Ok(SecretKey::from_str(&privkey).expect("parse privkey"))
}

fn user_address_deser<'de, D>(deserializer: D) -> Result<Address, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let address: String = serde::Deserialize::deserialize(deserializer)?;
    Ok(Address::from_str(&address).expect("parse address"))
}

pub fn load_config(path: PathBuf) -> eyre::Result<Config> {
    let file = fs::read_to_string(path)?;
    Ok(toml::from_str(&file)?)
}
