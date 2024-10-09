use clap::Parser;
use eyre::Result;
use regex::{Captures, Regex};
use sourcemap::SourceMap;
use std::env::current_dir;
use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, BufReader, Stdin};
use std::path::PathBuf;

#[derive(Parser, Debug)]
struct Args {
    /// Path to source map
    sourcemap: String,
    /// Path to traceback containing mysterious WASM address to resolve, default to read from stdin
    input: Option<String>,
    /// Print filtered result to stdout instead of stderr
    #[arg(short = 'o', long)]
    stdout: bool,
    /// Use absolute path to source files resolved instead of relative to current working directory
    #[arg(short = 'p', long)]
    absolute_path: bool,
}

#[derive(Debug)]
enum Input {
    File(BufReader<File>),
    Stdin(Stdin),
}

impl Input {
    fn open(input: Option<String>) -> Result<Self> {
        let this = match input {
            Some(input) => Self::File(BufReader::new(OpenOptions::new().read(true).open(input)?)),
            None => Self::Stdin(io::stdin()),
        };
        Ok(this)
    }

    fn read_line(&mut self, mut buf: &mut String) -> Result<usize> {
        Ok(match self {
            Input::File(file) => file.read_line(&mut buf)?,
            Input::Stdin(stdin) => stdin.read_line(&mut buf)?,
        })
    }
}

fn resolve(map: &SourceMap, addr: &str, cwd: &Option<PathBuf>) -> Option<String> {
    let addr = if addr.starts_with("0x") {
        usize::from_str_radix(addr.strip_prefix("0x").unwrap(), 16).ok()?
    } else {
        addr.parse().ok()?
    };
    let token = map.lookup_token(0, addr as u32)?;
    let path = match token.get_source() {
        Some(s) => match cwd {
            Some(cwd) => {
                let path = PathBuf::from(s);
                match path.strip_prefix(cwd) {
                    Ok(path) => path.to_str().unwrap_or(s).to_owned(),
                    Err(_) => s.to_owned(),
                }
            }
            None => s.to_owned(),
        },
        None => "<unknown>".to_string(),
    };
    Some(format!(
        "{path}:{}:{}",
        token.get_src_line() + 1,
        token.get_src_col() + 1
    ))
}

fn main() -> Result<()> {
    let args = Args::parse();
    let map = SourceMap::from_reader(OpenOptions::new().read(true).open(args.sourcemap)?)?;
    let cwd = if args.absolute_path {
        None
    } else {
        Some(current_dir()?)
    };
    let mut input = Input::open(args.input)?;
    // wasm://wasm/000c5502:wasm-function[1060]:0x2648d
    let re = Regex::new(r"wasm\://.*\:.*\:((?:0x)?[[:xdigit:]]+)")?;
    let mut buf = String::new();
    loop {
        buf.clear();
        if input.read_line(&mut buf)? == 0 {
            break;
        }
        let result = re.replace_all(&buf, |caps: &Captures| {
            format!(
                "{} {}",
                &caps[0],
                resolve(&map, &caps[1], &cwd).unwrap_or_default()
            )
        });
        if args.stdout {
            print!("{result}")
        } else {
            eprint!("{result}")
        }
    }
    Ok(())
}
