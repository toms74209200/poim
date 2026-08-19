use crate::cli::Args;
use crate::convert::{self, ExtractedImage};
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
    })?;

    let Some(directory) = &args.images else {
        return Ok(());
    };

    for image in &conversion.images {
        write_image(directory, image)?;
    }

    Ok(())
}

fn write_image(directory: &str, image: &ExtractedImage) -> Result<(), RunError> {
    let path = std::path::Path::new(directory).join(image.path.as_str());

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| RunError::Write {
            path: parent.display().to_string(),
            source,
        })?;
    }

    std::fs::write(&path, &image.data).map_err(|source| RunError::Write {
        path: path.display().to_string(),
        source,
    })
}
