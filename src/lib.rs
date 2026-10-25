pub mod archive;
pub mod auth;
pub mod config;
pub mod covers;
pub mod edit;
pub mod library;
pub mod lookup;
pub mod metadata;
pub mod opds;
pub mod paths;
pub mod reader;
pub mod store;
pub mod templates;
pub mod watch;
pub mod web;

pub use config::Config;
pub use web::router;
