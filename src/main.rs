//! GNU-compatible `base64` command for Windows (reference: coreutils 8.32).

use std::ffi::OsString;
use std::fs::File;
use std::io::{self, Read, Write};
use std::mem::ManuallyDrop;
use std::os::windows::io::{AsRawHandle, FromRawHandle};
use std::process::ExitCode;

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const DEFAULT_WRAP: usize = 76;
/// Multiple of 3, so only the last chunk of the input can need padding.
const ENCODE_CHUNK: usize = 48 * 1024;
const DECODE_CHUNK: usize = 64 * 1024;
const NOT_BASE64: u8 = 0xFF;

const DECODE_TABLE: [u8; 256] = {
    let mut table = [NOT_BASE64; 256];
    let mut i = 0;
    while i < ALPHABET.len() {
        table[ALPHABET[i] as usize] = i as u8;
        i += 1;
    }
    table
};

const HELP: &str = "\
Usage: base64 [OPTION]... [FILE]
Base64 encode or decode FILE, or standard input, to standard output.

With no FILE, or when FILE is -, read standard input.

Mandatory arguments to long options are mandatory for short options too.
  -d, --decode          decode data
  -i, --ignore-garbage  when decoding, ignore non-alphabet characters
  -w, --wrap=COLS       wrap encoded lines after COLS character (default 76).
                          Use 0 to disable line wrapping

      --help     display this help and exit
      --version  output version information and exit

The data are encoded as described for the base64 alphabet in RFC 4648.
When decoding, the input may contain newlines in addition to the bytes of
the formal base64 alphabet.  Use --ignore-garbage to attempt to recover
from any other non-alphabet bytes in the encoded stream.
";

/// Order matters: it is the order GNU lists possibilities for an ambiguous option.
const LONG_OPTIONS: [&str; 5] = ["decode", "wrap", "ignore-garbage", "help", "version"];

#[derive(Default)]
struct Config {
    decode: bool,
    ignore_garbage: bool,
    wrap: Option<usize>,
    file: Option<OsString>,
}

enum Command {
    Run(Config),
    Help,
    Version,
}

struct CliError {
    message: String,
    show_hint: bool,
}

fn usage_error(message: String) -> CliError {
    CliError {
        message,
        show_hint: true,
    }
}

enum Failure {
    Read(io::Error),
    Write(io::Error),
    InvalidInput,
}

/// Mirrors getopt_long: options and operands may be mixed, short options may be
/// clustered, long options may be abbreviated, and `--help`/`--version` act as
/// soon as they are reached.
fn parse_args(args: impl IntoIterator<Item = OsString>) -> Result<Command, CliError> {
    let mut config = Config::default();
    let mut operands: Vec<OsString> = Vec::new();
    let mut args = args.into_iter();
    let mut options_ended = false;

    while let Some(arg) = args.next() {
        let text = arg.to_string_lossy().into_owned();
        if options_ended || text == "-" || !text.starts_with('-') {
            operands.push(arg);
        } else if text == "--" {
            options_ended = true;
        } else if let Some(long) = text.strip_prefix("--") {
            if let Some(command) = parse_long(long, &mut args, &mut config)? {
                return Ok(command);
            }
        } else {
            parse_short_cluster(&text[1..], &mut args, &mut config)?;
        }
    }

    let mut operands = operands.into_iter();
    config.file = operands.next();
    if let Some(extra) = operands.next() {
        return Err(usage_error(format!(
            "extra operand '{}'",
            extra.to_string_lossy()
        )));
    }
    Ok(Command::Run(config))
}

fn parse_long(
    long: &str,
    args: &mut impl Iterator<Item = OsString>,
    config: &mut Config,
) -> Result<Option<Command>, CliError> {
    let (name, value) = match long.split_once('=') {
        Some((name, value)) => (name, Some(value)),
        None => (long, None),
    };
    let matches: Vec<&str> = LONG_OPTIONS
        .into_iter()
        .filter(|option| option.starts_with(name))
        .collect();
    let option = match matches.as_slice() {
        [] => return Err(usage_error(format!("unrecognized option '--{long}'"))),
        [option] => *option,
        all => {
            let possibilities: Vec<String> = all.iter().map(|o| format!("'--{o}'")).collect();
            return Err(usage_error(format!(
                "option '--{long}' is ambiguous; possibilities: {}",
                possibilities.join(" ")
            )));
        }
    };

    if option == "wrap" {
        let cols = match value {
            Some(value) => value.to_owned(),
            None => match args.next() {
                Some(next) => next.to_string_lossy().into_owned(),
                None => {
                    return Err(usage_error(
                        "option '--wrap' requires an argument".to_owned(),
                    ));
                }
            },
        };
        config.wrap = Some(parse_wrap(&cols)?);
        return Ok(None);
    }
    if value.is_some() {
        return Err(usage_error(format!(
            "option '--{option}' doesn't allow an argument"
        )));
    }
    match option {
        "decode" => config.decode = true,
        "ignore-garbage" => config.ignore_garbage = true,
        "help" => return Ok(Some(Command::Help)),
        _ => return Ok(Some(Command::Version)),
    }
    Ok(None)
}

fn parse_short_cluster(
    cluster: &str,
    args: &mut impl Iterator<Item = OsString>,
    config: &mut Config,
) -> Result<(), CliError> {
    for (index, option) in cluster.char_indices() {
        match option {
            'd' => config.decode = true,
            'i' => config.ignore_garbage = true,
            'w' => {
                let attached = &cluster[index + 1..];
                let cols = if !attached.is_empty() {
                    attached.to_owned()
                } else if let Some(next) = args.next() {
                    next.to_string_lossy().into_owned()
                } else {
                    return Err(usage_error("option requires an argument -- 'w'".to_owned()));
                };
                config.wrap = Some(parse_wrap(&cols)?);
                return Ok(());
            }
            other => return Err(usage_error(format!("invalid option -- '{other}'"))),
        }
    }
    Ok(())
}

/// Same rules as xdectoumax with no suffixes: leading whitespace and `+` are
/// accepted, any `-` is rejected (even `-0`), and values above usize::MAX are an
/// overflow error with its own message.
fn parse_wrap(text: &str) -> Result<usize, CliError> {
    let invalid = |detail: &str| CliError {
        message: format!("invalid wrap size: '{text}'{detail}"),
        show_hint: false,
    };
    let trimmed = text.trim_start_matches([' ', '\t', '\n', '\r', '\x0B', '\x0C']);
    let digits = trimmed.strip_prefix('+').unwrap_or(trimmed);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(invalid(""));
    }
    digits
        .parse()
        .map_err(|_| invalid(": Value too large for defined data type"))
}

/// Fills `buf` unless EOF comes first, so that short reads from pipes do not
/// produce padding in the middle of the stream.
fn read_full(input: &mut impl Read, buf: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match input.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(read) => filled += read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    Ok(filled)
}

fn encode_into(data: &[u8], encoded: &mut Vec<u8>) {
    for group in data.chunks(3) {
        let b0 = group[0];
        let b1 = group.get(1).copied().unwrap_or(0);
        let b2 = group.get(2).copied().unwrap_or(0);
        encoded.push(ALPHABET[usize::from(b0 >> 2)]);
        encoded.push(ALPHABET[usize::from((b0 & 0x03) << 4 | b1 >> 4)]);
        encoded.push(if group.len() > 1 {
            ALPHABET[usize::from((b1 & 0x0F) << 2 | b2 >> 6)]
        } else {
            b'='
        });
        encoded.push(if group.len() > 2 {
            ALPHABET[usize::from(b2 & 0x3F)]
        } else {
            b'='
        });
    }
}

fn encode_stream(input: &mut impl Read, out: &mut impl Write, wrap: usize) -> Result<(), Failure> {
    let mut data = vec![0u8; ENCODE_CHUNK];
    let mut encoded = Vec::with_capacity(ENCODE_CHUNK / 3 * 4);
    let mut wrapped = Vec::new();
    let mut column = 0;

    loop {
        let read = read_full(input, &mut data).map_err(Failure::Read)?;
        if read == 0 {
            break;
        }
        encoded.clear();
        encode_into(&data[..read], &mut encoded);

        if wrap == 0 {
            out.write_all(&encoded).map_err(Failure::Write)?;
        } else {
            wrapped.clear();
            let mut rest = encoded.as_slice();
            while !rest.is_empty() {
                if column == wrap {
                    wrapped.push(b'\n');
                    column = 0;
                }
                let take = rest.len().min(wrap - column);
                wrapped.extend_from_slice(&rest[..take]);
                column += take;
                rest = &rest[take..];
            }
            out.write_all(&wrapped).map_err(Failure::Write)?;
        }
        if read < data.len() {
            break;
        }
    }
    if column > 0 {
        out.write_all(b"\n").map_err(Failure::Write)?;
    }
    Ok(())
}

fn is_alphabet(byte: u8) -> bool {
    DECODE_TABLE[usize::from(byte)] != NOT_BASE64
}

/// Decodes up to four symbols, emitting every byte that is valid before a
/// failure, which is the partial output GNU writes ahead of "invalid input".
fn decode_group(group: &[u8], decoded: &mut Vec<u8>) -> bool {
    let value = |index: usize| DECODE_TABLE[usize::from(group[index])];

    if group.len() < 2 || !is_alphabet(group[0]) || !is_alphabet(group[1]) {
        return false;
    }
    decoded.push(value(0) << 2 | value(1) >> 4);
    if group.len() == 2 {
        return false;
    }
    if group[2] == b'=' {
        return group.len() == 4 && group[3] == b'=';
    }
    if !is_alphabet(group[2]) {
        return false;
    }
    decoded.push(value(1) << 4 | value(2) >> 2);
    if group.len() == 3 {
        return false;
    }
    if group[3] == b'=' {
        return true;
    }
    if !is_alphabet(group[3]) {
        return false;
    }
    decoded.push(value(2) << 6 | value(3));
    true
}

fn decode_stream(
    input: &mut impl Read,
    out: &mut impl Write,
    ignore_garbage: bool,
) -> Result<(), Failure> {
    let mut data = vec![0u8; DECODE_CHUNK];
    let mut decoded = Vec::with_capacity(DECODE_CHUNK / 4 * 3);
    let mut group = [0u8; 4];
    let mut pending = 0;

    loop {
        let read = read_full(input, &mut data).map_err(Failure::Read)?;
        decoded.clear();
        let mut valid = true;
        for &byte in &data[..read] {
            let skip = if ignore_garbage {
                !is_alphabet(byte) && byte != b'='
            } else {
                byte == b'\n'
            };
            if skip {
                continue;
            }
            group[pending] = byte;
            pending += 1;
            if pending == group.len() {
                pending = 0;
                if !decode_group(&group, &mut decoded) {
                    valid = false;
                    break;
                }
            }
        }
        if valid && read < data.len() && pending > 0 {
            valid = decode_group(&group[..pending], &mut decoded);
        }
        out.write_all(&decoded).map_err(Failure::Write)?;
        if !valid {
            return Err(Failure::InvalidInput);
        }
        if read < data.len() {
            return Ok(());
        }
    }
}

/// Unbuffered stdout on the raw handle. `io::stdout()` refuses non-UTF-8 bytes
/// when attached to a console, which would break decoding binary data.
fn raw_stdout() -> ManuallyDrop<File> {
    // SAFETY: the handle stays valid for the whole process, and ManuallyDrop
    // keeps File from closing a handle it does not own.
    ManuallyDrop::new(unsafe { File::from_raw_handle(io::stdout().as_raw_handle()) })
}

/// GNU wording for the common cases, instead of the localized Windows message.
fn system_message(error: &io::Error) -> String {
    match error.kind() {
        io::ErrorKind::NotFound => "No such file or directory".to_owned(),
        io::ErrorKind::PermissionDenied => "Permission denied".to_owned(),
        _ => error.to_string(),
    }
}

fn fail(message: &str) -> ExitCode {
    eprintln!("base64: {message}");
    ExitCode::FAILURE
}

fn process(input: &mut impl Read, config: &Config) -> ExitCode {
    let mut out = raw_stdout();
    let result = if config.decode {
        decode_stream(input, &mut *out, config.ignore_garbage)
    } else {
        let wrap = config.wrap.unwrap_or(DEFAULT_WRAP);
        encode_stream(input, &mut *out, wrap)
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::InvalidInput) => fail("invalid input"),
        Err(Failure::Read(error)) => fail(&format!("read error: {}", system_message(&error))),
        // A closed pipe (`base64 big | head`) is what SIGPIPE ends quietly on Linux.
        Err(Failure::Write(error)) if error.kind() == io::ErrorKind::BrokenPipe => {
            ExitCode::FAILURE
        }
        Err(Failure::Write(error)) => fail(&format!("write error: {}", system_message(&error))),
    }
}

fn print_and_exit(text: &str) -> ExitCode {
    match raw_stdout().write_all(text.as_bytes()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => ExitCode::FAILURE,
        Err(error) => fail(&format!("write error: {}", system_message(&error))),
    }
}

fn main() -> ExitCode {
    let config = match parse_args(std::env::args_os().skip(1)) {
        Ok(Command::Run(config)) => config,
        Ok(Command::Help) => return print_and_exit(HELP),
        Ok(Command::Version) => {
            return print_and_exit(concat!("base64 ", env!("CARGO_PKG_VERSION"), "\n"));
        }
        Err(error) => {
            eprintln!("base64: {}", error.message);
            if error.show_hint {
                eprintln!("Try 'base64 --help' for more information.");
            }
            return ExitCode::FAILURE;
        }
    };

    match config.file.as_deref() {
        Some(path) if path != "-" => {
            let name = path.to_string_lossy();
            if path_is_directory(path) {
                return fail("read error: Is a directory");
            }
            match File::open(path) {
                Ok(mut file) => process(&mut file, &config),
                Err(error) => fail(&format!("{name}: {}", system_message(&error))),
            }
        }
        _ => process(&mut io::stdin().lock(), &config),
    }
}

fn path_is_directory(path: &std::ffi::OsStr) -> bool {
    std::fs::metadata(path).is_ok_and(|metadata| metadata.is_dir())
}
