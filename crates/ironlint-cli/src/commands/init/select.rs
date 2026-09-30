//! Plain confirmation for the setup plan, without changing terminal state.
use anyhow::Result;
use std::io::{BufRead, Write};

pub fn confirm<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
    default_yes: bool,
) -> Result<bool> {
    let choices = if default_yes { "Y/n" } else { "y/N" };
    write!(writer, "  Proceed? [{choices}] ")?;
    writer.flush()?;
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        return Ok(false);
    }
    Ok(match line.trim().to_ascii_lowercase().as_str() {
        "" => default_yes,
        "y" | "yes" => true,
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, Cursor};

    #[test]
    fn answers_and_empty_line_obey_the_selected_default() {
        for default in [false, true] {
            for (answer, expected) in [
                ("\n", default),
                (" y \n", true),
                ("YES\n", true),
                ("n\n", false),
                ("No\n", false),
                ("cancel\n", false),
                ("invalid\n", false),
            ] {
                let mut output = Vec::new();
                assert_eq!(
                    confirm(&mut Cursor::new(answer), &mut output, default).unwrap(),
                    expected
                );
                assert_eq!(
                    String::from_utf8(output).unwrap(),
                    if default {
                        "  Proceed? [Y/n] "
                    } else {
                        "  Proceed? [y/N] "
                    }
                );
            }
        }
    }

    #[test]
    fn eof_never_accepts_even_a_yes_default() {
        for default in [false, true] {
            assert!(!confirm(&mut io::empty(), &mut Vec::new(), default).unwrap());
        }
    }

    #[test]
    fn input_and_output_errors_are_reported() {
        struct FailedInput;
        impl io::Read for FailedInput {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::other("input closed"))
            }
        }
        impl BufRead for FailedInput {
            fn fill_buf(&mut self) -> io::Result<&[u8]> {
                Err(io::Error::other("input closed"))
            }
            fn consume(&mut self, _: usize) {}
        }
        assert!(confirm(&mut FailedInput, &mut Vec::new(), true).is_err());
        assert!(confirm(&mut Cursor::new("yes\n"), &mut io::Cursor::new([]), true).is_err());
    }
}
