//! Execution permission for command sources (E-9.2).
//!
//! Whether a registered command may run during verify/check follows a strict
//! precedence, highest first:
//!   1. the invocation's `--run-command[=bool]` flag
//!   2. user config `VERIFY_COMMAND_AUTO_RUN`
//!   3. built-in `false`
//! One call's flag never carries into another call; loading or rebuilding
//! metadata is never execution consent.

/// Inputs to the decision. `None` = that layer didn't specify.
#[derive(Debug, Default)]
pub struct RunChoice {
    /// This invocation's `--run-command` (`Some(true)`/`Some(false)`).
    pub cli: Option<bool>,
    /// User config `VERIFY_COMMAND_AUTO_RUN`.
    pub config: Option<bool>,
}

/// May the command run this invocation? Built-in floor is `false`.
pub fn may_run(c: &RunChoice) -> bool {
    c.cli.or(c.config).unwrap_or(false)
}
