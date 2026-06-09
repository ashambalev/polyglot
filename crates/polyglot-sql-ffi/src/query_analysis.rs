use crate::helpers::{err_result, map_polyglot_error, ok_json_result, panic_result, required_arg};
use crate::types::{PolyglotResult, STATUS_PARSE_ERROR, STATUS_SERIALIZATION_ERROR};
use polyglot_sql::{analyze_query as core_analyze_query, AnalyzeQueryOptions};
use std::os::raw::c_char;

/// Return compact query analysis facts for a SELECT or set operation.
///
/// `options_json` must be a JSON object compatible with `AnalyzeQueryOptions`.
#[no_mangle]
pub extern "C" fn polyglot_analyze_query(
    sql: *const c_char,
    options_json: *const c_char,
) -> PolyglotResult {
    match std::panic::catch_unwind(|| analyze_query_impl(sql, options_json)) {
        Ok(result) => result,
        Err(panic) => panic_result(panic),
    }
}

fn analyze_query_impl(sql: *const c_char, options_json: *const c_char) -> PolyglotResult {
    let sql = match unsafe { required_arg(sql, "sql") } {
        Ok(value) => value,
        Err(result) => return result,
    };
    let options = match parse_analyze_query_options(options_json) {
        Ok(options) => options,
        Err(result) => return result,
    };

    match core_analyze_query(&sql, options) {
        Ok(result) => ok_json_result(&result),
        Err(error) => err_result(
            map_polyglot_error(&error, STATUS_PARSE_ERROR),
            error.to_string(),
        ),
    }
}

fn parse_analyze_query_options(
    options_json: *const c_char,
) -> Result<AnalyzeQueryOptions, PolyglotResult> {
    let options_json = unsafe { required_arg(options_json, "options_json") }?;

    serde_json::from_str::<AnalyzeQueryOptions>(&options_json).map_err(|error| {
        err_result(
            STATUS_SERIALIZATION_ERROR,
            format!("Invalid analyze_query options JSON: {error}"),
        )
    })
}
