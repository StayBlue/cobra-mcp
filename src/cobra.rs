//! Safe wrapper around the C++ bridge to CoBRA.

use rmcp::schemars::{self, JsonSchema};
use serde::{Deserialize, Serialize};

#[cxx::bridge(namespace = "cobra_mcp")]
mod ffi {
    enum Status {
        Simplified,
        Unsupported,
        Error,
    }

    struct SimplifyRequest {
        expr: String,
        bitwidth: u32,
        max_vars: u32,
    }

    struct SimplifyResponse {
        status: Status,
        expr: String,
        input_vars: Vec<String>,
        semantic_class: String,
        spot_checked: bool,
        message: String,
    }

    unsafe extern "C++" {
        include!("cobra-mcp/cpp/bridge.h");

        fn simplify(request: &SimplifyRequest) -> SimplifyResponse;
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Request {
    pub expr: String,
    pub bitwidth: u32,
    pub max_vars: u32,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// CoBRA found a simpler equivalent expression.
    Simplified,
    /// CoBRA could not simplify the expression; `expr` is the input as CoBRA
    /// normalized it.
    Unsupported,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct Simplification {
    pub status: Status,
    /// The resulting expression.
    pub expr: String,
    /// Variables in the input, sorted.
    pub input_variables: Vec<String>,
    /// CoBRA's structural classification of the input: linear, semilinear,
    /// polynomial, or non-polynomial.
    pub semantic_class: String,
    /// Whether the result passed CoBRA's random-input spot check.
    pub spot_checked: bool,
    /// Why CoBRA could not simplify the expression.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

pub fn simplify(request: &Request) -> Result<Simplification, String> {
    let response = ffi::simplify(&ffi::SimplifyRequest {
        expr: request.expr.clone(),
        bitwidth: request.bitwidth,
        max_vars: request.max_vars,
    });
    let status = match response.status {
        ffi::Status::Simplified => Status::Simplified,
        ffi::Status::Unsupported => Status::Unsupported,
        _ => return Err(response.message),
    };
    Ok(Simplification {
        status,
        expr: response.expr,
        input_variables: response.input_vars,
        semantic_class: response.semantic_class,
        spot_checked: response.spot_checked,
        reason: Some(response.message).filter(|m| !m.is_empty()),
    })
}
