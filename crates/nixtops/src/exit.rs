//! Exit codes of the CLI (protocol section 12) and what a command hands back to `main`.

/// 0 pass; 1 gate failed; 2 config invalid; 3 environment error; 4 refused by policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exit {
    Pass,
    GateFailed,
    ConfigInvalid,
    Environment,
    Refused,
}

impl Exit {
    pub fn code(self) -> u8 {
        match self {
            Exit::Pass => 0,
            Exit::GateFailed => 1,
            Exit::ConfigInvalid => 2,
            Exit::Environment => 3,
            Exit::Refused => 4,
        }
    }
}

/// The result of a command: the exit code and the text for stdout and stderr.
#[derive(Debug, PartialEq, Eq)]
pub struct Output {
    pub exit: Exit,
    pub stdout: String,
    pub stderr: String,
}

impl Output {
    pub fn ok(stdout: String) -> Self {
        Self {
            exit: Exit::Pass,
            stdout,
            stderr: String::new(),
        }
    }

    pub fn fail(exit: Exit, stderr: String) -> Self {
        Self {
            exit,
            stdout: String::new(),
            stderr,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_follow_the_cli_table() {
        let all = [
            Exit::Pass,
            Exit::GateFailed,
            Exit::ConfigInvalid,
            Exit::Environment,
            Exit::Refused,
        ];
        let codes: Vec<u8> = all.iter().map(|e| e.code()).collect();
        assert_eq!(codes, [0, 1, 2, 3, 4]);
    }

    #[test]
    fn output_constructors_fill_the_right_stream() {
        let ok = Output::ok("a".into());
        assert_eq!(
            (ok.exit, ok.stdout.as_str(), ok.stderr.as_str()),
            (Exit::Pass, "a", "")
        );
        let fail = Output::fail(Exit::Refused, "b".into());
        assert_eq!(
            (fail.exit, fail.stdout.as_str(), fail.stderr.as_str()),
            (Exit::Refused, "", "b")
        );
    }
}
