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
