//! Central place for all TUI default values.
//! Update these and the whole app picks them up.

pub struct Defaults;

impl Defaults {
    /* Keygen */
    pub const KEYGEN_COUNT: &'static str = "1";
    pub const KEYGEN_SAVE_TO_FILE: bool = false;
    pub const KEYGEN_OUT_PATH: &'static str = "./outputFiles/keys.json";

    /* Batch */
    pub const BATCH_INPUT_PATH: &'static str = "./inputFiles/my_input.json";
    pub const BATCH_OUTPUT_PATH: &'static str = "./outputFiles/batch_output.json";
    pub const BATCH_GAS_LIMIT: &'static str = "30000000";
    pub const BATCH_MAX_FEE_PER_GAS: &'static str = "30000000000";
    pub const BATCH_MAX_PRIORITY_FEE_PER_GAS: &'static str = "2000000000";

    /* Create Key Pair */
    pub const CREATE_KEYPAIR_OUT_DIR: &'static str = "./generated_private_keys";

    /* Create Transaction */
    pub const CREATE_DELEGATION_OUT_DIR: &'static str = "./generated_transactions";

    pub const DELEGATION_INPUT_DIR: &'static str = "./input_files";


    /* Decryption */
    pub const DECRYPT_OUTPUT_DIR: &'static str = "./decrypted_files";

    /* Global chain/tx defaults (used by Create Delegation page and elsewhere) */
    pub const CHAIN_ID: u64 = 31337;
    pub const CONTRACT_ADDRESS: &'static str =
        "0x5FbDB2315678afecb367f032d93F642f64180aa3";
    pub const GAS_LIMIT: &'static str = "200000";
    pub const MAX_FEE_PER_GAS: &'static str = "30000000000"; // 30 gwei
    pub const MAX_PRIORITY_FEE_PER_GAS: &'static str = "2000000000"; // 2 gwei
}
