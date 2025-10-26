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
}
