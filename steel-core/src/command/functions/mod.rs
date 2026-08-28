//! Datapack-loaded `.mcfunction` command functions.
//!
//! Vanilla parity: `ServerFunctionLibrary` and `ServerFunctionManager`, reading
//! the same `data/<namespace>/function/**.mcfunction` and
//! `data/<namespace>/tags/function/**.json` layout out of the datapack
//! directory. Vanilla keeps one function library per server; Steel does the
//! same, so a function is callable from every domain.

mod library;
mod loader;
mod manager;
mod parser;

#[cfg(test)]
mod tests;

pub(crate) use library::CommandFunction;
pub(crate) use manager::{FunctionManager, FunctionReloadReport};
