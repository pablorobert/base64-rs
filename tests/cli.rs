use std::io::Write;
use std::process::{Command, Output, Stdio};
use std::thread;

fn run(args: &[&str], input: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_base64"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let input = input.to_vec();
    // Separate thread: large inputs would deadlock against a full stdout pipe.
    let writer = thread::spawn(move || {
        // The process may legitimately exit before reading everything.
        let _ = stdin.write_all(&input);
    });
    let output = child.wait_with_output().unwrap();
    writer.join().unwrap();
    output
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn encodes_rfc4648_vectors() {
    let vectors: [(&[u8], &[u8]); 7] = [
        (b"", b""),
        (b"f", b"Zg==\n"),
        (b"fo", b"Zm8=\n"),
        (b"foo", b"Zm9v\n"),
        (b"foob", b"Zm9vYg==\n"),
        (b"fooba", b"Zm9vYmE=\n"),
        (b"foobar", b"Zm9vYmFy\n"),
    ];
    for (plain, encoded) in vectors {
        assert_eq!(run(&[], plain).stdout, encoded);
    }
}

#[test]
fn wraps_at_76_by_default_without_blank_line_on_exact_multiple() {
    let output = run(&[], &[b'a'; 57]).stdout;
    assert_eq!(output.len(), 77);
    assert_eq!(output[76], b'\n');
}

#[test]
fn wrap_options_are_equivalent() {
    let expected = b"Zm9v\nYmFy\n";
    for args in [
        &["-w4"][..],
        &["-w", "4"],
        &["--wrap=4"],
        &["--wrap", "4"],
        &["--wr=4"],
    ] {
        assert_eq!(run(args, b"foobar").stdout, expected, "{args:?}");
    }
}

#[test]
fn wrap_zero_has_no_trailing_newline() {
    assert_eq!(run(&["-w0"], b"foobar").stdout, b"Zm9vYmFy");
}

#[test]
fn decode_ignores_newlines_and_accepts_concatenated_padding() {
    assert_eq!(run(&["-d"], b"Zm9v\nYmFy\n").stdout, b"foobar");
    assert_eq!(run(&["-d"], b"Zg==Zg==").stdout, b"ff");
}

#[test]
fn invalid_input_keeps_partial_output_and_fails() {
    let output = run(&["-d"], b"Zm9v!!!!");
    assert_eq!(output.stdout, b"foo");
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(stderr(&output).trim_end(), "base64: invalid input");
}

#[test]
fn decode_accepts_what_cmd_echo_sends() {
    // `echo texto | base64` encodes "texto \r\n"; `echo <that> | base64 -d` adds
    // the same space and CRLF after the encoded text.
    for input in [
        &b"dGV4dG8gDQo=\r\n"[..],
        b"dGV4dG8gDQo= \r\n",
        b"dGV4dG8gDQo=  \r\n",
        b"dGV4dG8gDQo= ",
        b"dGV4dG8gDQo=\r",
    ] {
        let output = run(&["-d"], input);
        assert_eq!(output.stdout, b"texto \r\n", "{input:?}");
        assert!(output.status.success(), "{input:?}");
    }
}

#[test]
fn decode_accepts_crlf_between_lines() {
    let output = run(&["-d"], b"Zm9v\r\nYmFy\r\n");
    assert_eq!(output.stdout, b"foobar");
    assert!(output.status.success());
}

#[test]
fn decode_handles_trailing_blanks_across_chunk_boundaries() {
    let plain: Vec<u8> = (0..1_000_003u32).map(|i| (i % 251) as u8).collect();
    let encoded = String::from_utf8(run(&[], &plain).stdout).unwrap();
    let output = run(&["-d"], encoded.replace('\n', " \r\n").as_bytes());
    assert_eq!(output.stdout, plain);
    assert!(output.status.success());
}

#[test]
fn blank_before_more_data_is_still_invalid() {
    for (input, partial) in [
        (&b"Zm9v Zm9v"[..], &b"foo"[..]),
        (b"Zm9v\rZm9v", b"foo"),
        (b"Zm9 v", b"fo"),
    ] {
        let output = run(&["-d"], input);
        assert_eq!(output.stdout, partial, "{input:?}");
        assert_eq!(output.status.code(), Some(1), "{input:?}");
    }
}

#[test]
fn missing_padding_is_still_invalid_as_in_gnu() {
    let output = run(&["-d"], b"dGV4dG8gDQo \r\n");
    assert_eq!(output.stdout, b"texto \r\n");
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn ignore_garbage_still_discards_blanks_everywhere() {
    let output = run(&["-d", "-i"], b"Zm 9v\r\nYm\rFy");
    assert_eq!(output.stdout, b"foobar");
    assert!(output.status.success());
}

#[test]
fn clustered_and_abbreviated_options_work() {
    assert_eq!(run(&["-di"], b"Z!g==").stdout, b"f");
    assert_eq!(run(&["--dec", "--ig"], b"Z!g==").stdout, b"f");
}

#[test]
fn binary_data_round_trips_across_chunk_boundaries() {
    let plain: Vec<u8> = (0..1_000_003u32).map(|i| (i % 251) as u8).collect();
    let encoded = run(&[], &plain).stdout;
    assert_eq!(run(&["-d"], &encoded).stdout, plain);
}

#[test]
fn usage_errors_match_gnu_messages() {
    let cases: [(&[&str], &str); 6] = [
        (&["-x"], "base64: invalid option -- 'x'"),
        (&["--foo"], "base64: unrecognized option '--foo'"),
        (&["-w"], "base64: option requires an argument -- 'w'"),
        (&["--wrap"], "base64: option '--wrap' requires an argument"),
        (
            &["--decode=1"],
            "base64: option '--decode' doesn't allow an argument",
        ),
        (&["a", "b"], "base64: extra operand 'b'"),
    ];
    for (args, message) in cases {
        let output = run(args, b"");
        assert_eq!(output.status.code(), Some(1), "{args:?}");
        assert_eq!(
            stderr(&output),
            format!("{message}\nTry 'base64 --help' for more information.\n"),
            "{args:?}"
        );
    }
}

#[test]
fn invalid_wrap_has_no_hint() {
    for value in [
        "x",
        "-1",
        "-0",
        " -0",
        "5k",
        "",
        "+",
        "99999999999999999999x",
    ] {
        let output = run(&["--wrap", value], b"");
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(
            stderr(&output),
            format!("base64: invalid wrap size: '{value}'\n")
        );
    }
}

#[test]
fn wrap_limit_is_usize_max() {
    let largest = run(&["--wrap=18446744073709551615"], b"foobar");
    assert_eq!(largest.stdout, b"Zm9vYmFy\n");

    let output = run(&["--wrap=18446744073709551616"], b"foobar");
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stderr(&output),
        "base64: invalid wrap size: '18446744073709551616': \
         Value too large for defined data type\n"
    );
}

#[test]
fn ambiguous_option_lists_possibilities_in_gnu_order() {
    let output = run(&["--=x"], b"");
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stderr(&output),
        "base64: option '--=x' is ambiguous; possibilities: \
         '--decode' '--wrap' '--ignore-garbage' '--help' '--version'\n\
         Try 'base64 --help' for more information.\n"
    );
}

#[test]
fn missing_file_reports_gnu_message() {
    let output = run(&["does-not-exist.bin"], b"");
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stderr(&output),
        "base64: does-not-exist.bin: No such file or directory\n"
    );
}

#[test]
fn dash_reads_stdin_and_double_dash_ends_options() {
    assert_eq!(run(&["-"], b"foo").stdout, b"Zm9v\n");
    assert_eq!(run(&["--", "-"], b"foo").stdout, b"Zm9v\n");
}

#[test]
fn help_and_version_exit_successfully() {
    let help = run(&["--help"], b"");
    assert!(help.status.success());
    assert!(
        String::from_utf8_lossy(&help.stdout).starts_with("Usage: base64 [OPTION]... [FILE]\n")
    );
    let version = run(&["--version"], b"");
    assert!(version.status.success());
    assert!(String::from_utf8_lossy(&version.stdout).starts_with("base64 "));
}
