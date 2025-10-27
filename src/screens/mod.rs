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
pub mod create_delegation;
pub mod create_revocation;
pub mod create_redelegation;
pub mod create_permanent_invalidation;
pub mod decrypt_file;                 // already added
pub mod select_file_for_decryption;   // NEW
pub mod decrypt_file_details;         // NEW

// Re-exports (so callers can use crate::screens::XxxScreen)
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
pub use select_file_for_decryption::SelectFileForDecryptionScreen; // NEW
pub use decrypt_file_details::DecryptFileDetailsScreen;             // NEW

// Re-export the confirmation screen type
pub mod confirm_ok;
pub use confirm_ok::{ConfirmOkScreen, AfterOk};

