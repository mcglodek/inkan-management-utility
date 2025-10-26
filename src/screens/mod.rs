pub mod main_menu;
pub mod keygen;
pub mod batch;
pub mod confirm_quit;
pub mod result;

// NEW: add these three lines so Rust knows the files exist
pub mod create_inkan_identity;
pub mod recover_inkan_identity;
pub mod advanced_tools;

// Re-exports so you can keep using crate::screens::XYZScreen
pub use main_menu::MainMenuScreen;
pub use keygen::KeygenScreen;
pub use batch::BatchScreen;
pub use confirm_quit::ConfirmQuitScreen;
pub use result::ResultScreen;

// NEW: re-export the three new dummy pages
pub use create_inkan_identity::CreateInkanIdentityScreen;
pub use recover_inkan_identity::RecoverInkanIdentityScreen;
pub use advanced_tools::AdvancedToolsScreen;

