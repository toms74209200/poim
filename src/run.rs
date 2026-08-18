use crate::cli::Args;
use crate::convert;
use crate::epub::EpubError;

#[derive(Debug)]
pub enum RunError {
    Read {
        path: String,
        source: std::io::Error,
    },
    Convert(EpubError),
    Write {
        path: String,
        source: std::io::Error,
    },
}

impl core::fmt::Display for RunError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            RunError::Read { path, source } => write!(f, "cannot read {path}: {source}"),
            RunError::Convert(error) => write!(f, "{error}"),
            RunError::Write { path, source } => write!(f, "cannot write {path}: {source}"),
        }
    }
}

pub fn run(args: &Args) -> Result<(), RunError> {
    let data = std::fs::read(&args.input).map_err(|source| RunError::Read {
        path: args.input.clone(),
        source,
    })?;

    let conversion = convert::convert_epub(&data).map_err(RunError::Convert)?;

    std::fs::write(&args.output, &conversion.markdown).map_err(|source| RunError::Write {
        path: args.output.clone(),
        source,
    })
}
