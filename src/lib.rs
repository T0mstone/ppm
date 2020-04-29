#![warn(missing_docs)]
//! ppm is a templating and macro engine / library
//!
//! It works with *commands* which are sequences that start with `%`
//! - basic commands take the form `%cmd(...)` or `%cmd`
//! - block commands take the form `%cmd(...)%{ ... %}` or `%cmd%{ ... %}`
//!
//! Character escaping is intelligent (at least in the predefined commands),
//! i.e. you can write `\:` and it will only be converted to `:` when `:` itelf needs to be escaped
//!
//! Command names may not contain whitespace characters or any of the characters `%(){}`

pub use crate::invoke::{BasicCommandArgs, BlockCommandArgs};
use crate::util::{FileLoc, Span};
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::mem::take;
use std::path::PathBuf;

mod invoke;

mod shell_util;
mod util;

///
pub mod predefined_commands;

/// issues are problems encountered.
///
/// ppm has a philosophy of always allowing you to have an end result (be it empty),
/// thus there are no real "errors" in the sense that they stop execution.
/// This means that all issues are handled as warnings , even if they are errors
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Issue {
    /// An identifier for the issue. It is not unique, an example is `command:missing_args`
    pub id: &'static str,
    /// A message that describes the issue
    pub msg: String,
    /// The region the issue occurred in
    pub span: Span,
}

/// A helper struct for displaying [`Issue`s](struct.Issue.html)
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct IssueDisplay<'a> {
    id: &'static str,
    msg: &'a str,
    start: FileLoc,
    end: FileLoc,
}

impl<'a> Display for IssueDisplay<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "[{} at {}...{}] {}",
            self.id, self.start, self.end, self.msg
        ))
    }
}

impl Issue {
    /// Creates an issue from an IO error
    ///
    /// (something that occurred often enough in the predefined commands for this function to exist)
    #[inline]
    pub fn io_error(e: std::io::Error, span: Span, extra: Option<&str>) -> Self {
        let extra = extra.map_or(String::new(), |s| format!(" {}", s));
        Self {
            id: "io_error",
            msg: format!("IO Error{}: {}", extra, e),
            span,
        }
    }

    /// Creates a value that can be formatted by [`fmt::Display`](https://doc.rust-lang.org/std/fmt/trait.Display.html)
    #[inline]
    pub fn display(&self, original_src: &str) -> IssueDisplay {
        let (start, end) = self.span.start_end_loc(original_src);
        IssueDisplay {
            id: self.id,
            msg: &self.msg,
            start,
            end,
        }
    }
}

#[inline]
fn absorb_new_issues(issues: &mut Vec<Issue>, subspan: Span, new_issues: Vec<Issue>) {
    issues.extend(new_issues.into_iter().map(|mut e| {
        e.span.start += subspan.start;
        debug_assert!(e.span.end() < subspan.end(), "malformed subspan");
        e
    }));
}

/// The type of a handler function for basic commands
pub type BasicHandler = fn(BasicCommandArgs, &mut Engine) -> String;
/// The type of a handler function for block commands
pub type BlockHandler = fn(BlockCommandArgs, &mut Engine) -> String;

/// The main type.
#[derive(Clone)]
pub struct Engine {
    vars: HashMap<String, String>,
    root_path: Option<PathBuf>,
    basic_commands: HashMap<String, BasicHandler>,
    block_commands: HashMap<String, BlockHandler>,
}

/// Command names may not contain whitespace characters or any of the characters `%(){}`
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct InvalidCommandName(String);

impl Engine {
    /// Creates a new engine which knows the given variables, but no commands
    #[inline]
    pub fn new(vars: HashMap<String, String>) -> Self {
        Self {
            vars,
            root_path: None,
            basic_commands: HashMap::new(),
            block_commands: HashMap::new(),
        }
    }

    /// Creates a new engine which knows the given variables and the predefined commands
    #[inline]
    pub fn with_predefined_commands(vars: HashMap<String, String>) -> Self {
        let mut res = Self::new(vars);
        res.add_basic_commands(predefined_commands::basic::get_all_handlers())
            .expect("internal error: default basic command names nonconformal");
        res.add_block_commands(predefined_commands::block::get_all_handlers())
            .expect("internal error: default block command names nonconformal");
        res
    }

    /// Sets the root path (the path all relative paths are relative to)
    ///
    /// If unset, the root path defaults to the current working directory
    #[inline]
    pub fn with_root_path(mut self, path: PathBuf) -> Self {
        self.root_path = Some(path);
        self
    }

    /// Unsets the root path (the path all relative paths are relative to)
    ///
    /// If unset, the root path defaults to the current working directory
    #[inline]
    pub fn without_root_path(mut self) -> Self {
        self.root_path = None;
        self
    }

    /// Tests if a command name is valid
    pub fn is_valid_command_name(s: &str) -> bool {
        !s.contains(|c: char| c.is_whitespace() || "%(){}".contains(c))
    }

    /// Adds a basic command to the engine's knowledge
    #[inline]
    pub fn add_basic_command(
        &mut self,
        cmd: &str,
        handler: BasicHandler,
    ) -> Result<Option<BasicHandler>, InvalidCommandName> {
        if Self::is_valid_command_name(cmd) {
            Ok(self.basic_commands.insert(cmd.to_string(), handler))
        } else {
            Err(InvalidCommandName(cmd.to_string()))
        }
    }

    /// Adds a block command to the engine's knowledge
    #[inline]
    pub fn add_block_command<'a>(
        &mut self,
        cmd: &str,
        handler: BlockHandler,
    ) -> Result<Option<BlockHandler>, InvalidCommandName> {
        if Self::is_valid_command_name(cmd) {
            Ok(self.block_commands.insert(cmd.to_string(), handler))
        } else {
            Err(InvalidCommandName(cmd.to_string()))
        }
    }

    /// Adds multiple basic commands to the engine's knowledge
    #[inline]
    pub fn add_basic_commands<I: IntoIterator<Item = (String, BasicHandler)>>(
        &mut self,
        iter: I,
    ) -> Result<(), InvalidCommandName> {
        for (k, v) in iter.into_iter() {
            self.add_basic_command(&k, v)?;
        }
        Ok(())
    }

    /// Adds multiple block commands to the engine's knowledge
    #[inline]
    pub fn add_block_commands<I: IntoIterator<Item = (String, BlockHandler)>>(
        &mut self,
        iter: I,
    ) -> Result<(), InvalidCommandName> {
        for (k, v) in iter.into_iter() {
            self.add_block_command(&k, v)?;
        }
        Ok(())
    }

    /// processes a string, pushing all encountered issues onto `issues`
    pub fn process(&mut self, mut s: String, issues: &mut Vec<Issue>) -> String {
        let mut changes = vec![];
        let mut offs = 0;
        while let Some(di) = s[offs..].find('%') {
            offs += di;
            let i = offs + 1;
            let sl = &s[i..];

            let block_commands = take(&mut self.block_commands);
            match invoke::invoke_block_handlers(sl, i, &block_commands, self) {
                Some((len, res, mut new_issues)) => {
                    changes.push((offs, len, res));
                    offs += len;
                    issues.append(&mut new_issues);
                    continue;
                }
                None => (),
            }
            self.block_commands = block_commands;

            let basic_commands = take(&mut self.basic_commands);
            match invoke::invoke_basic_handlers(sl, i, &basic_commands, self) {
                Some((len, res, mut new_issues)) => {
                    changes.push((offs, len, res));
                    offs += len;
                    issues.append(&mut new_issues);
                    continue;
                }
                None => (),
            }
            self.basic_commands = basic_commands;

            issues.push(Issue {
                id: "command:unknown",
                msg: format!(
                    "invalid or unknown command at {} (starting with {}...)",
                    FileLoc::from_index(offs, &s),
                    if s[offs..].len() < 10 {
                        &s[offs..]
                    } else {
                        &s[offs..offs + 10]
                    }
                ),
                span: Span::new(offs, 1),
            });

            // move one over to find the next `%`
            offs += 1;
        }
        // reverse it to avoid index-shifting
        changes.reverse();
        for (offs, len, res) in changes {
            s.replace_range(offs..offs + len, &res);
        }
        s
    }

    #[inline]
    /// Creates a new `Vec` to hold issues, before calling [`self.process`](#method.process)
    pub fn process_new(&mut self, template: String) -> (String, Vec<Issue>) {
        let mut issues = Vec::new();

        (self.process(template, &mut issues), issues)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vars() {
        let s = "abc%(var)def%(var2)ghi";
        let mut vars = HashMap::new();
        vars.insert("var".to_string(), "1".to_string());
        vars.insert("var2".to_string(), "2".to_string());
        let mut en = Engine::with_predefined_commands(vars);
        let (s, i) = en.process_new(s.to_string());
        assert_eq!(i, vec![]);
        assert_eq!(&s, "abc1def2ghi");
    }

    #[test]
    fn test_run() {
        let s = "%run(echo \"test 123\"456 789)";
        let vars = HashMap::new();
        let mut en = Engine::with_predefined_commands(vars);
        let (s, i) = en.process_new(s.to_string());
        assert_eq!(i, vec![]);
        assert_eq!(&s, "test 123456 789\n");
    }

    #[test]
    fn test_alt() {
        let s = "%alt(:::a)%alt(::a:b:c)";
        let vars = HashMap::new();
        let mut en = Engine::with_predefined_commands(vars);
        let (s, i) = en.process_new(s.to_string());
        assert_eq!(i, vec![]);
        assert_eq!(&s, "aa");
    }

    #[test]
    fn test_lit() {
        let s = "%lit(%alt(abcs))";
        let vars = HashMap::new();
        let mut en = Engine::with_predefined_commands(vars);
        let (s, i) = en.process_new(s.to_string());
        assert_eq!(i, vec![]);
        assert_eq!(&s, "%alt(abcs)");
    }

    #[test]
    fn test_invalid() {
        let s = "abc%invalid[[[lsdfga7vyev%lit()sd";
        let vars = HashMap::new();
        let mut en = Engine::with_predefined_commands(vars);
        let (s, i) = en.process_new(s.to_string());
        assert_eq!(
            i,
            vec![Issue {
                id: "command:unknown",
                msg: "invalid or unknown command at 1:4 (starting with %invalid[[...)".to_string(),
                span: Span::new(3, 1)
            }]
        );
        assert_eq!(&s, "abc%invalid[[[lsdfga7vyevsd");
    }

    #[test]
    fn test_unknown_var() {
        let s = "abc%(unknown)";
        let vars = HashMap::new();
        let mut en = Engine::with_predefined_commands(vars);
        let (s, i) = en.process_new(s.to_string());
        assert_eq!(
            i,
            vec![Issue {
                id: "command:invalid_args",
                msg: "unknown variable: unknown".to_string(),
                span: Span::new(3, 10)
            }]
        );
        assert_eq!(&s, "abc");
    }
}
