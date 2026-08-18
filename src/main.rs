use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let parsed = match poim::cli::parse(&args) {
        Ok(parsed) => parsed,
        Err(error) => return fail(&error),
    };

    match poim::run::run(&parsed) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => fail(&error),
    }
}

fn fail(error: &dyn core::fmt::Display) -> ExitCode {
    eprintln!("poim: {error}");
    ExitCode::FAILURE
}
