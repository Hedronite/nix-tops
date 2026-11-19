//! Every command of the CLI table exists; one that is not built yet says so and exits 4.

mod common;
use common::{nixtops, stderr};

const UNBUILT: [&[&str]; 2] = [&["baseline"], &["laws"]];

#[test]
fn an_unbuilt_command_exits_4_and_names_itself() {
    for args in UNBUILT {
        let out = nixtops(args);
        assert_eq!(out.status.code(), Some(4), "{args:?}");
        assert_eq!(
            stderr(&out),
            format!("nixtops {}: not built yet\n", args[0]),
            "{args:?}"
        );
        assert!(out.stdout.is_empty(), "{args:?}");
    }
}

#[test]
fn flags_of_the_table_parse() {
    let unbuilt: &[&[&str]] = &[
        &["laws", "list"],
        &["laws", "run"],
        &["laws", "explain", "N-04"],
    ];
    for args in unbuilt {
        assert_eq!(nixtops(args).status.code(), Some(4), "{args:?}");
    }
    // Built commands: the flags parse (`--help` last makes clap stop after parsing them).
    let built: &[&[&str]] = &[
        &[
            "gate",
            "--max-tier",
            "2",
            "--base",
            "main",
            "--keep-going",
            "--json",
            "--help",
        ],
        &[
            "gate",
            "--only",
            "eval",
            "--system",
            "x86_64-linux",
            "--dry-run",
            "--evaluator",
            "host",
            "--help",
        ],
        &["report", "--format", "json", "--help"],
    ];
    for args in built {
        assert_eq!(nixtops(args).status.code(), Some(0), "{args:?}");
    }
}

#[test]
fn an_unknown_command_is_a_usage_error() {
    let out = nixtops(&["frobnicate"]);
    assert_eq!(out.status.code(), Some(2));
}
