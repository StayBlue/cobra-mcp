// Copyright (c) 2026 StayBlue
// SPDX-License-Identifier: MIT

//! The MCP server and its tools.

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::schemars::{self, JsonSchema};
use rmcp::transport::stdio;
use rmcp::{Json, ServiceExt, tool, tool_handler, tool_router};
use serde::Deserialize;
use tokio::sync::Semaphore;

use crate::cobra::{Request, Simplification};
use crate::worker;

const DEFAULT_BITWIDTH: u32 = 64;
const DEFAULT_MAX_VARS: u32 = 16;
/// The signature vector CoBRA evaluates has 2^max_vars entries.
const MAX_MAX_VARS: u32 = 20;
const DEFAULT_TIMEOUT_MS: u64 = 30_000;
const MAX_TIMEOUT_MS: u64 = 600_000;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SimplifyParams {
    /// The MBA expression, e.g. `(x&y)+(x|y)`. Supports `+ - * **`,
    /// `& | ^ ~`, `<<` and `>>` by constants, parentheses, decimal and `0x`
    /// hex constants, and identifiers as variables.
    pub expr: String,
    /// Width of the modular arithmetic in bits, 1-64. Defaults to 64.
    pub bitwidth: Option<u32>,
    /// Maximum number of distinct variables, 1-20. Defaults to 16.
    pub max_vars: Option<u32>,
    /// Time limit in milliseconds, up to 600000. Defaults to 30000.
    pub timeout_ms: Option<u64>,
}

#[derive(Clone)]
pub struct CobraServer {
    tool_router: ToolRouter<Self>,
    /// Limits concurrent workers to the number of CPUs.
    workers: Arc<Semaphore>,
}

#[tool_router]
impl CobraServer {
    pub fn new() -> Self {
        let cpus = thread::available_parallelism().map_or(1, |n| n.get());
        Self {
            tool_router: Self::tool_router(),
            workers: Arc::new(Semaphore::new(cpus)),
        }
    }

    #[tool(
        description = "Simplify a Mixed Boolean-Arithmetic (MBA) expression, as produced by \
                       code obfuscators, into an equivalent simpler expression using CoBRA. \
                       Results are checked on random inputs."
    )]
    async fn simplify(
        &self,
        Parameters(params): Parameters<SimplifyParams>,
    ) -> Result<Json<Simplification>, String> {
        let bitwidth = params.bitwidth.unwrap_or(DEFAULT_BITWIDTH);
        if !(1..=64).contains(&bitwidth) {
            return Err(format!("bitwidth must be between 1 and 64, got {bitwidth}"));
        }
        let max_vars = params.max_vars.unwrap_or(DEFAULT_MAX_VARS);
        if !(1..=MAX_MAX_VARS).contains(&max_vars) {
            return Err(format!(
                "max_vars must be between 1 and {MAX_MAX_VARS}, got {max_vars}"
            ));
        }
        let timeout_ms = params.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);
        if !(1..=MAX_TIMEOUT_MS).contains(&timeout_ms) {
            return Err(format!(
                "timeout_ms must be between 1 and {MAX_TIMEOUT_MS}, got {timeout_ms}"
            ));
        }

        let request = Request {
            expr: params.expr,
            bitwidth,
            max_vars,
        };
        let _permit = self.workers.acquire().await.map_err(|e| e.to_string())?;
        worker::simplify(&request, Duration::from_millis(timeout_ms))
            .await
            .map(Json)
    }
}

#[tool_handler(
    router = self.tool_router,
    name = "cobra-mcp",
    instructions = "Deobfuscates Mixed Boolean-Arithmetic (MBA) expressions with CoBRA \
                    (https://github.com/trailofbits/CoBRA). Pass the bitwidth of the \
                    original code's integer type; results depend on it."
)]
impl rmcp::ServerHandler for CobraServer {}

pub async fn serve() -> Result<(), Box<dyn std::error::Error>> {
    let service = CobraServer::new().serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
