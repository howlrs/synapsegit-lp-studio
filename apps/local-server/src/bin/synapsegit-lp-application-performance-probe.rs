#![forbid(unsafe_code)]

use serde_json::json;
use std::process::ExitCode;
use synapsegit_lp_local_server::application_performance::run_change_set_performance_probe;

fn parse_count(value: Option<String>) -> Result<usize, &'static str> {
    value
        .ok_or("argument_value_required")?
        .parse::<usize>()
        .map_err(|_| "argument_value_invalid")
}

fn arguments() -> Result<(usize, usize), &'static str> {
    let mut warmups = 2;
    let mut samples = 20;
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--warmup-samples" => warmups = parse_count(arguments.next())?,
            "--samples" => samples = parse_count(arguments.next())?,
            _ => return Err("unknown_argument"),
        }
    }
    Ok((warmups, samples))
}

fn main() -> ExitCode {
    let result = arguments().and_then(|(warmups, samples)| {
        run_change_set_performance_probe(warmups, samples)
            .map_err(|_| "change_set_performance_probe_failed")
    });
    match result {
        Ok(result) => match serde_json::to_string(&result) {
            Ok(encoded) => {
                println!("{encoded}");
                ExitCode::SUCCESS
            }
            Err(_) => ExitCode::FAILURE,
        },
        Err(error_code) => {
            println!("{}", json!({ "status": "failed", "errorCode": error_code }));
            ExitCode::FAILURE
        }
    }
}
