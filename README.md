# base64 for Windows

**English** | [Português (Brasil)](README.pt-BR.md)

A native `base64` command for Windows, written in Rust with no dependencies. It has the same flags, error messages and exit codes as GNU coreutils 8.32 on Linux.

Scripts that use `base64` on Linux work the same way, without WSL, Git Bash or MSYS2.

```console
$ echo -n "olá, mundo" | base64
b2zDoSwgbXVuZG8=

$ echo b2zDoSwgbXVuZG8= | base64 -d
olá, mundo
```

## Installation

You need [Rust](https://rustup.rs) and the MSVC linker, which comes with the Visual Studio Build Tools.

```console
git clone https://github.com/pablorobert/base64-rs.git
cd base64-rs
cargo install --path .
```

The executable goes to `%USERPROFILE%\.cargo\bin\base64.exe`. If another `base64` is already on your `PATH`, such as Git's, check which one runs with `where base64`.

To compile only, run `cargo build --release`. The binary is written to `target\release\base64.exe`.

## Usage

```text
base64 [OPTION]... [FILE]
```

With no `FILE`, or when `FILE` is `-`, it reads standard input.

| Option | Description |
| --- | --- |
| `-d`, `--decode` | Decode data |
| `-i`, `--ignore-garbage` | When decoding, ignore non-alphabet characters |
| `-w`, `--wrap=COLS` | Wrap encoded lines after `COLS` characters (default: 76; `0` disables wrapping) |
| `--help` | Display help and exit |
| `--version` | Display version and exit |

Options follow `getopt_long` rules: they can be clustered (`-di`), abbreviated (`--dec`) and mixed with the file (`base64 file.b64 -d`). `--` ends the options.

### Examples

```console
# Encode a file on a single line
base64 -w0 photo.png

# Decode to a file
base64 -d photo.b64 > photo.png

# Wrap lines every 4 characters
base64 -w4 file.txt

# Decode a file with CRLF line endings
base64 -d windows-file.b64
```

On error, the exit code is `1`. With invalid data, everything valid before the error is written, followed by `base64: invalid input` on stderr, as in GNU.

## Windows caveats

- **Trailing spaces and CRLF:** unlike GNU, `-d` accepts spaces and `\r` at the end of a line or of the input, because `cmd`'s `echo` sends `text= \r\n` for `echo text= | base64 -d`. Inside the data they still cause `invalid input`. Use `-di` to discard them anywhere.
- **Windows PowerShell 5.1:** `>` and `|` between native commands re-encode bytes and can corrupt binary data. Use `cmd`, Git Bash or PowerShell 7.4+ instead.
- **Console output:** `-d` writes bytes without conversion. If accented characters look wrong, enable UTF-8 with `chcp 65001`.
- **WSL interop:** running a `.exe` from WSL with stdin and stdout on large files can hang. This also happened with Git's `base64.exe`, so the cause is not in this project. Run the binary directly on Windows.

## Differences from GNU

| Situation | GNU on Linux | This project |
| --- | --- | --- |
| `--version` | coreutils text | `base64 0.1.0` |
| `--help` | Includes GNU links in the footer | Same text, without the footer |
| Quotes in error messages | Depend on locale (`‘x’` in UTF-8) | Always `'x'`, as with `LC_ALL=C` |
| Closed pipe (`base64 big \| head`) | Terminated by `SIGPIPE` | Exits silently with code `1` |

## Tests

```console
cargo test
```

The suite in `tests/cli.rs` covers the RFC 4648 vectors, line wrapping, decoding, error messages and binary data crossing buffer boundaries.

The initial version was also compared with `base64` from GNU coreutils 8.32 on Ubuntu 22.04: all 420 cases had identical stdout, stderr and exit codes, with `LC_ALL=C`. They include random binaries of up to 1 MB, malformed input and option errors. The comparison script is not part of the repository, and use in a real console window has not been tested.

## License

Distributed under the [MIT License](LICENSE).
