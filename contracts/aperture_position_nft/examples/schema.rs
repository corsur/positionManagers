use std::env::current_dir;
use std::fs::create_dir_all;

use aperture_common::nft::Extension;
use aperture_position_nft::msg::MigrateMsg;
use cosmwasm_schema::{export_schema, remove_schemas, schema_for};
pub use cw721_base::{InstantiateMsg, QueryMsg};

pub type ExecuteMsg = cw721_base::ExecuteMsg<Extension>;

fn main() {
    let mut out_dir = current_dir().unwrap();
    out_dir.push("schema");
    create_dir_all(&out_dir).unwrap();
    remove_schemas(&out_dir).unwrap();

    export_schema(&schema_for!(InstantiateMsg), &out_dir);
    export_schema(&schema_for!(ExecuteMsg), &out_dir);
    export_schema(&schema_for!(QueryMsg), &out_dir);
    export_schema(&schema_for!(MigrateMsg), &out_dir);
}