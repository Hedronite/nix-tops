//! Running a program and collecting its output, with one retry rule: a program that was written
//! a moment ago can still be open for writing in a child another thread forked, and `exec` then
//! fails with "text file busy" (`ETXTBSY`, Linux). That is a race, not an answer, so the run is
//! tried again a few times. Any other error is returned as it is.

use std::{
    io::{self, ErrorKind},
    process::{Command, Output},
    thread,
    time::Duration,
};

/// How many times a busy program is tried, and the pause between tries.
const ATTEMPTS: u32 = 40;
const PAUSE: Duration = Duration::from_millis(25);

fn is_busy(error: &io::Error) -> bool {
    error.kind() == ErrorKind::ExecutableFileBusy
}

/// Calls `run`, and again while it fails with "busy", `ATTEMPTS` calls in all, pausing in between.
/// The loop is bounded by its range, so no change to the checks inside it can make it run forever.
fn retry<T>(mut run: impl FnMut() -> io::Result<T>, pause: Duration) -> io::Result<T> {
    for _ in 1..ATTEMPTS {
        match run() {
            Err(error) if is_busy(&error) => thread::sleep(pause),
            other => return other,
        }
    }
    run()
}

/// `command.output()`, tried again while the program is busy.
pub fn output(command: &mut Command) -> io::Result<Output> {
    retry(|| command.output(), PAUSE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn busy() -> io::Error {
        io::Error::from(ErrorKind::ExecutableFileBusy)
    }

    #[test]
    fn only_a_busy_executable_counts_as_busy() {
        assert!(is_busy(&busy()));
        assert!(!is_busy(&io::Error::from(ErrorKind::NotFound)));
    }

    #[test]
    fn a_busy_program_is_tried_again_until_it_runs() {
        let mut calls = 0;
        let result = retry(
            || {
                calls += 1;
                if calls < 3 { Err(busy()) } else { Ok(7) }
            },
            Duration::ZERO,
        );
        assert_eq!((result.unwrap(), calls), (7, 3));
    }

    #[test]
    fn a_program_that_stays_busy_gives_up_after_the_attempts_and_says_busy() {
        let mut calls = 0;
        let result: io::Result<()> = retry(
            || {
                calls += 1;
                Err(busy())
            },
            Duration::ZERO,
        );
        assert!(is_busy(&result.unwrap_err()));
        assert_eq!(calls, ATTEMPTS);
    }

    #[test]
    fn any_other_error_is_returned_at_once() {
        let mut calls = 0;
        let result: io::Result<()> = retry(
            || {
                calls += 1;
                Err(io::Error::from(ErrorKind::NotFound))
            },
            Duration::ZERO,
        );
        assert_eq!(result.unwrap_err().kind(), ErrorKind::NotFound);
        assert_eq!(calls, 1);
    }

    #[test]
    fn output_runs_a_program_and_returns_its_status() {
        let out = output(Command::new("/bin/sh").args(["-c", "echo hi; exit 3"])).unwrap();
        assert_eq!(
            (
                out.status.code(),
                String::from_utf8_lossy(&out.stdout).trim()
            ),
            (Some(3), "hi")
        );
        assert_eq!(
            output(&mut Command::new("/nonexistent/tool"))
                .unwrap_err()
                .kind(),
            ErrorKind::NotFound
        );
    }
}
