// Core screens
pub mod main_menu;
pub mod keygen;
pub mod batch;
pub mod confirm_quit;
pub mod result;

// Intro / identity flows
pub mod create_inkan_identity;
pub mod recover_inkan_identity;
pub mod advanced_tools;

// Advanced Tools -> Create* pages
pub mod create_key_pair;
pub mod create_delegation;                // canonical Create Delegation screen (manual input)
pub mod create_revocation;                // canonical Create Revocation screen (manual input)
pub mod create_redelegation;              // canonical Create Re-Delegation screen (manual input)
pub mod create_permanent_invalidation;

// Decrypt flow
pub mod decrypt_file;                     // already added
pub mod select_file_for_decryption;       // NEW
pub mod decrypt_file_details;             // NEW

// Load-from-file flows (delegation)
pub mod choose_delegation_info_dir;
pub mod select_delegation_info_file;

// Load-from-file flows (revocation)
pub mod choose_revocation_info_dir;
pub mod select_revocation_info_file;

// Load-from-file flows (re-delegation)
pub mod choose_redelegation_info_dir;
pub mod select_redelegation_info_file;

// Load-from-file flows (permanent invalidation)
pub mod choose_permanent_invalidation_info_dir;
pub mod select_permanent_invalidation_info_file;

// ---------------- Re-exports ----------------
pub use main_menu::MainMenuScreen;
pub use keygen::KeygenScreen;
pub use batch::BatchScreen;
pub use confirm_quit::ConfirmQuitScreen;
pub use result::ResultScreen;

pub use create_inkan_identity::CreateInkanIdentityScreen;
pub use recover_inkan_identity::RecoverInkanIdentityScreen;
pub use advanced_tools::AdvancedToolsScreen;

pub use create_key_pair::CreateKeyPairScreen;
pub use create_delegation::CreateDelegationScreen;
pub use create_revocation::CreateRevocationScreen;
pub use create_redelegation::CreateRedelegationScreen;
pub use create_permanent_invalidation::CreatePermanentInvalidationScreen;

pub use decrypt_file::DecryptFileScreen;
pub use select_file_for_decryption::SelectFileForDecryptionScreen;
pub use decrypt_file_details::DecryptFileDetailsScreen;

pub use choose_delegation_info_dir::ChooseDelegationInfoDirScreen;
pub use select_delegation_info_file::SelectDelegationInfoFileScreen;

pub use choose_revocation_info_dir::ChooseRevocationInfoDirScreen;
pub use select_revocation_info_file::SelectRevocationInfoFileScreen;

pub use choose_redelegation_info_dir::ChooseRedelegationInfoDirScreen;
pub use select_redelegation_info_file::SelectRedelegationInfoFileScreen;

pub use choose_permanent_invalidation_info_dir::ChoosePermanentInvalidationInfoDirScreen;
pub use select_permanent_invalidation_info_file::SelectPermanentInvalidationInfoFileScreen;

// Re-export the confirmation screen type
pub mod confirm_ok;
pub use confirm_ok::{ConfirmOkScreen, AfterOk};

// Legacy/removed modules (Option B cleanup):
// pub mod manually_input_delegation_info; // removed
