#[derive(Debug, PartialEq)]
pub struct Args {
    pub input: String,
    pub output: String,
    pub images: Option<String>,
}

#[derive(Debug, PartialEq)]
pub enum CliError {
    MissingInput,
    MissingOutput,
    MissingOutputValue,
    MissingImagesValue,
    UnknownOption(String),
}

impl core::fmt::Display for CliError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            CliError::MissingInput => write!(f, "missing input file"),
            CliError::MissingOutput => write!(f, "missing -o <output>"),
            CliError::MissingOutputValue => write!(f, "-o requires a value"),
            CliError::MissingImagesValue => write!(f, "--images requires a value"),
            CliError::UnknownOption(s) => write!(f, "unknown option: {s}"),
        }
    }
}

pub fn parse(args: &[String]) -> Result<Args, CliError> {
    let mut input = None;
    let mut output = None;
    let mut images = None;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "-o" => {
                i += 1;
                if i >= args.len() {
                    return Err(CliError::MissingOutputValue);
                }
                output = Some(args[i].clone());
            }
            "--images" => {
                i += 1;
                if i >= args.len() {
                    return Err(CliError::MissingImagesValue);
                }
                images = Some(args[i].clone());
            }
            s if s.starts_with('-') => {
                return Err(CliError::UnknownOption(s.to_string()));
            }
            _ => {
                input = Some(args[i].clone());
            }
        }
        i += 1;
    }

    Ok(Args {
        input: input.ok_or(CliError::MissingInput)?,
        output: output.ok_or(CliError::MissingOutput)?,
        images,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &[&str]) -> Vec<String> {
        s.iter().map(|v| v.to_string()).collect()
    }

    #[test]
    fn minimal_args() {
        let result = parse(&args(&["book.epub", "-o", "out.md"]));
        assert_eq!(
            result,
            Ok(Args {
                input: "book.epub".to_string(),
                output: "out.md".to_string(),
                images: None,
            })
        );
    }

    #[test]
    fn with_images_option() {
        let result = parse(&args(&["book.epub", "-o", "out.md", "--images", "img/"]));
        assert_eq!(
            result,
            Ok(Args {
                input: "book.epub".to_string(),
                output: "out.md".to_string(),
                images: Some("img/".to_string()),
            })
        );
    }

    #[test]
    fn options_before_input() {
        let result = parse(&args(&["-o", "out.md", "--images", "img/", "book.epub"]));
        assert_eq!(
            result,
            Ok(Args {
                input: "book.epub".to_string(),
                output: "out.md".to_string(),
                images: Some("img/".to_string()),
            })
        );
    }

    #[test]
    fn missing_input() {
        let result = parse(&args(&["-o", "out.md"]));
        assert_eq!(result, Err(CliError::MissingInput));
    }

    #[test]
    fn missing_output() {
        let result = parse(&args(&["book.epub"]));
        assert_eq!(result, Err(CliError::MissingOutput));
    }

    #[test]
    fn missing_output_value() {
        let result = parse(&args(&["book.epub", "-o"]));
        assert_eq!(result, Err(CliError::MissingOutputValue));
    }

    #[test]
    fn missing_images_value() {
        let result = parse(&args(&["book.epub", "-o", "out.md", "--images"]));
        assert_eq!(result, Err(CliError::MissingImagesValue));
    }

    #[test]
    fn unknown_option() {
        let result = parse(&args(&["book.epub", "-o", "out.md", "--verbose"]));
        assert_eq!(
            result,
            Err(CliError::UnknownOption("--verbose".to_string()))
        );
    }
}
