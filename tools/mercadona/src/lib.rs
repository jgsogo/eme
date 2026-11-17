//! Unofficial, read-only Mercadona catalogue and product-search tools.
//!
//! This crate encapsulates the HTTP integration with the Mercadona online shop
//! and its Algolia search index. It exposes product search, product details,
//! category browsing, and postal-code-to-warehouse resolution.
//!
//! The integration deliberately excludes authentication, shopping-cart
//! mutation, checkout, and order submission. Tool registration and schemas
//! follow the conventions of the EME agent platform; authorization and
//! execution policy remain responsibilities of agent-runtime.

mod arguments;
mod client;
mod get_category;
mod get_product;
mod list_categories;
mod resolve_postal_code;
mod search_products;

use std::path::PathBuf;

use agent_config::ConfigError;
use agent_runtime::ToolRegistry;

/// Register all Mercadona catalogue tools.
pub fn register_tools(registry: &mut ToolRegistry) -> Result<(), ConfigError> {
    let data = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data");

    registry.register_from_file(data.join("search_products.toml"), search_products::SearchProducts)?;
    registry.register_from_file(data.join("get_product.toml"), get_product::GetProduct)?;
    registry.register_from_file(data.join("list_categories.toml"), list_categories::ListCategories)?;
    registry.register_from_file(data.join("get_category.toml"), get_category::GetCategory)?;
    registry.register_from_file(
        data.join("resolve_postal_code.toml"),
        resolve_postal_code::ResolvePostalCode,
    )
}
