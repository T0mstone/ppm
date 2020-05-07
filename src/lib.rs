#![warn(missing_docs)]
//! ppm is a templating and macro engine / library
//!
//! It works with *commands* which are sequences that start with `(%` and end with `%)`
//! - The first part, the command id/head, is some string
//! - Then comes one whitespace character
//! - Then comes the body
//! - This means that a command looks like `(%cmd body%)`
//!
//! Character escaping is intelligent (at least in the predefined commands),
//! i.e. you can write `\:` and it will only be converted to `:` when `:` itelf needs to be escaped
//! - otherwise, it is kept as `\:`
//!
//! Command names may not contain whitespace characters or any of the characters `%(){}`

// pub use crate::invoke::{BasicCommandArgs, BlockCommandArgs};
pub use crate::util::{RowCol, Span};
use std::collections::{HashMap, HashSet};
use std::fmt::{Display, Formatter};
use std::marker::PhantomData;
use std::mem::{replace, take};
use std::path::PathBuf;
use tlib::iter_tools::{AutoEscape, IterSplit, Unescape};

// mod invoke;

mod shell_util;
mod util;

/// Predefined Commands
pub mod predefined_commands;
// /// Predefined Commands
// pub mod predefined_commands_old;

/// Issues are problems encountered.
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

/// A helper struct for displaying [`Issue`](struct.Issue.html)s
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct IssueDisplay<'a> {
    id: &'static str,
    msg: &'a str,
    start: RowCol,
    end: RowCol,
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

/// A struct representing the configuration for a command
#[derive(Debug)]
pub struct CommandConfig<'a, 'b> {
    /// The body of the command
    pub body: String,
    /// The span of the command body
    pub body_span: Span,
    /// The span of the whole command (including percent-parentheses)
    pub cmd_span: Span,
    /// A handle used to push issues onto
    pub issues: &'a mut Vec<Issue>,
    /// A handle used to modify the current engine state
    pub engine: &'b mut Engine<Captured>,
}

impl<'a, 'b> CommandConfig<'a, 'b> {
    /// Creates an issue with id `"command:missing_args"` and span `self.cmd_span`
    #[inline]
    pub fn missing_args(&self, msg: &str) -> Issue {
        Issue {
            id: "command:missing_args",
            msg: msg.to_string(),
            span: self.cmd_span,
        }
    }

    /// Creates an issue with id `"command:invalid_args"` and span `self.cmd_span`
    #[inline]
    pub fn invalid_args(&self, msg: String) -> Issue {
        Issue {
            id: "command:invalid_args",
            msg,
            span: self.body_span,
        }
    }

    /// Pushes an issue with id `"command:missing_args"` and span `self.cmd_span` onto `self.issues`
    #[inline]
    pub fn push_missing_args(&mut self, msg: &str) {
        self.issues.push(self.missing_args(msg))
    }

    /// Pushes an issue with id `"command:invalid_args"` and span `self.cmd_span` onto `self.issues`
    #[inline]
    pub fn push_invalid_args(&mut self, msg: String) {
        self.issues.push(self.invalid_args(msg))
    }

    #[inline]
    fn free_engine(&mut self) -> &'b mut Engine<Free> {
        unsafe { &mut *(self.engine as *mut _ as *mut Engine<Free>) }
    }

    /// Processes a string with `self.engine`
    #[inline]
    pub fn process(&mut self, s: String) -> String {
        let eng: &mut Engine<Free> = self.free_engine();
        let (res, is) = eng.process_new(s);
        absorb_new_issues(self.issues, self.body_span, is);
        res
    }

    /// Processes `self.body` with `self.engine`
    #[inline]
    pub fn process_body(&mut self) -> String {
        let body = take(&mut self.body);
        self.process(body)
    }

    /// Processes some provided portion of `self.body` with `self.engine`
    #[inline]
    pub fn process_subbody(&mut self, subbody: String, subspan: Span) -> Option<String> {
        let eng: &mut Engine<Free> = self.free_engine();
        let (res, is) = eng.process_new(subbody);
        let span = subspan.relative_to(&self.body_span)?;
        absorb_new_issues(self.issues, span, is);
        Some(res)
    }
}

// /// The type of a handler function for basic commands
// pub type BasicHandler = fn(BasicCommandArgs, &mut Engine) -> String;
// /// The type of a handler function for block commands
// pub type BlockHandler = fn(BlockCommandArgs, &mut Engine) -> String;
/// The type of a command handler function
pub type CommandHandler = fn(CommandConfig) -> String;

/// A typestate for an [`Engine`](struct.Engine.html) that is free, i.e. on its own
pub enum Free {}
/// A typestate for an [`Engine`](struct.Engine.html) that is captured, i.e. used inside some [`CommandConfig`](struct.CommandConfig.html)
pub enum Captured {}

/// The main type.
#[derive(Clone)]
pub struct Engine<State> {
    /// The variables stored in the engine
    pub vars: HashMap<String, String>,
    root_path: Option<PathBuf>,
    // basic_commands: HashMap<String, BasicHandler>,
    // block_commands: HashMap<String, BlockHandler>,
    commands: HashMap<String, CommandHandler>,
    _marker: PhantomData<State>,
}

impl<State> std::fmt::Debug for Engine<State> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine")
            .field("vars", &self.vars)
            .field("root_path", &self.root_path)
            // .field(
            //     "basic_commands",
            //     &self.basic_commands.keys().collect::<HashSet<_>>(),
            // )
            // .field(
            //     "block_commands",
            //     &self.block_commands.keys().collect::<HashSet<_>>(),
            // )
            .field("commands", &self.commands.keys().collect::<HashSet<_>>())
            .finish()
    }
}

/// Command names may not contain whitespace characters or any of the characters `%(){}`
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct InvalidCommandName(String);

impl<State> Engine<State> {
    /// Creates a new engine which knows the given variables, but no commands
    #[inline]
    pub fn new(vars: HashMap<String, String>) -> Self {
        Self {
            vars,
            root_path: None,
            // basic_commands: HashMap::new(),
            // block_commands: HashMap::new(),
            commands: HashMap::new(),
            _marker: PhantomData,
        }
    }

    /// Creates a new engine which knows the given variables and the predefined commands
    #[inline]
    pub fn with_predefined_commands(vars: HashMap<String, String>) -> Self {
        let mut res = Self::new(vars);
        // res.add_basic_commands(predefined_commands::basic::get_all_handlers())
        //     .expect("internal error: default basic command names nonconformal");
        // res.add_block_commands(predefined_commands::block::get_all_handlers())
        //     .expect("internal error: default block command names nonconformal");
        res.add_commands(predefined_commands::get_all_handlers())
            .expect("internal error: default command names invalid");
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

    /// Tests if a character may not appear inside a command name
    ///
    /// Invalid characters are whitespace or any of `%(){}`
    pub fn is_invalid_command_char(c: char) -> bool {
        c.is_whitespace() || "%(){}".contains(c)
    }

    /// Tests if a command name is valid (using [`is_invalid_command_char`](#method.is_invalid_command_char))
    pub fn is_valid_command_name(s: &str) -> bool {
        !s.contains(Self::is_invalid_command_char)
    }

    /// Adds a command to the engine's knowledge
    #[inline]
    pub fn add_command(
        &mut self,
        cmd: &str,
        handler: CommandHandler,
    ) -> Result<Option<CommandHandler>, InvalidCommandName> {
        if Self::is_valid_command_name(cmd) {
            Ok(self.commands.insert(cmd.to_string(), handler))
        } else {
            Err(InvalidCommandName(cmd.to_string()))
        }
    }

    /// Adds multiple commands to the engine's knowledge
    #[inline]
    pub fn add_commands<I: IntoIterator<Item = (String, CommandHandler)>>(
        &mut self,
        iter: I,
    ) -> Result<(), Vec<InvalidCommandName>> {
        let mut errs = vec![];
        for (k, v) in iter.into_iter() {
            if let Err(e) = self.add_command(&k, v) {
                errs.push(e)
            }
        }
        if errs.is_empty() {
            Ok(())
        } else {
            Err(errs)
        }
    }

    // /// Adds a basic command to the engine's knowledge
    // #[inline]
    // pub fn add_basic_command(
    //     &mut self,
    //     cmd: &str,
    //     handler: BasicHandler,
    // ) -> Result<Option<BasicHandler>, InvalidCommandName> {
    //     if Self::is_valid_command_name(cmd) {
    //         Ok(self.basic_commands.insert(cmd.to_string(), handler))
    //     } else {
    //         Err(InvalidCommandName(cmd.to_string()))
    //     }
    // }
    //
    // /// Adds a block command to the engine's knowledge
    // #[inline]
    // pub fn add_block_command(
    //     &mut self,
    //     cmd: &str,
    //     handler: BlockHandler,
    // ) -> Result<Option<BlockHandler>, InvalidCommandName> {
    //     if Self::is_valid_command_name(cmd) {
    //         Ok(self.block_commands.insert(cmd.to_string(), handler))
    //     } else {
    //         Err(InvalidCommandName(cmd.to_string()))
    //     }
    // }
    //
    // /// Adds multiple basic commands to the engine's knowledge
    // #[inline]
    // pub fn add_basic_commands<I: IntoIterator<Item = (String, BasicHandler)>>(
    //     &mut self,
    //     iter: I,
    // ) -> Result<(), InvalidCommandName> {
    //     for (k, v) in iter.into_iter() {
    //         self.add_basic_command(&k, v)?;
    //     }
    //     Ok(())
    // }
    //
    // /// Adds multiple block commands to the engine's knowledge
    // #[inline]
    // pub fn add_block_commands<I: IntoIterator<Item = (String, BlockHandler)>>(
    //     &mut self,
    //     iter: I,
    // ) -> Result<(), InvalidCommandName> {
    //     for (k, v) in iter.into_iter() {
    //         self.add_block_command(&k, v)?;
    //     }
    //     Ok(())
    // }
}

#[derive(Debug, Clone, Eq, PartialEq)]
enum SourceAtom {
    Str(String),
    Command(String),
}

fn parse_commands(mut s: String, issues: &mut Vec<Issue>) -> Vec<SourceAtom> {
    let mut res = vec![];
    let mut lvl = 0;
    let orig_s_len = s.len();
    'outer: while !s.is_empty() {
        let s_len = s.len();
        let mut iter = s.char_indices().auto_escape(|&(_, c)| c == '\\').peekable();
        while let Some((esc, (i, c))) = iter.next() {
            match c {
                '(' if !esc => {
                    if let Some(&(false, (_, '%'))) = iter.peek() {
                        lvl += 1;
                        if lvl == 1 {
                            let new_s = s.split_off(i);
                            let before = replace(&mut s, new_s);
                            s.replace_range(0..2, "");
                            res.push(SourceAtom::Str(before));
                            continue 'outer;
                        }
                    }
                }
                '%' if !esc => {
                    if let Some(&(false, (_, ')'))) = iter.peek() {
                        if lvl == 0 {
                            issues.push(Issue {
                                id: "command:unmatched_closing_delim",
                                msg: "Unmatched '%)'".to_string(),
                                span: Span::new(i, 2),
                            });
                            s.replace_range(i..i + 2, "");
                            continue 'outer;
                        }
                        lvl -= 1;
                        if lvl == 0 {
                            let new_s = s.split_off(i);
                            let before = replace(&mut s, new_s);
                            s.replace_range(0..2, "");
                            res.push(SourceAtom::Command(before));
                            continue 'outer;
                        }
                    }
                }
                _ => (),
            }
        }
        // reached the end
        if lvl == 0 {
            res.push(SourceAtom::Str(take(&mut s)));
        } else {
            issues.push(Issue {
                id: "command:no_end",
                msg: "Command has no end".to_string(),
                span: Span::new(orig_s_len - s_len, s_len),
            });
            res.push(SourceAtom::Command(take(&mut s)));
        }
    }
    res
}

impl Engine<Free> {
    #[inline]
    fn capture(&mut self) -> &mut Engine<Captured> {
        unsafe { &mut *(self as *mut _ as *mut Engine<Captured>) }
    }

    /// Processes a string, pushing any issues onto `issues`
    pub fn process(&mut self, s: String, issues: &mut Vec<Issue>) -> String {
        let orig_s = s.clone();
        let parsed = parse_commands(s, issues);
        let mut res = vec![];

        let mut start = 0;
        for atom in parsed {
            let is_command = matches!(atom, SourceAtom::Command(_));
            if is_command {
                start += 2;
            }

            let s_len;
            match atom {
                SourceAtom::Str(s) => {
                    s_len = s.len();
                    res.push(s);
                }
                SourceAtom::Command(s) => {
                    s_len = s.len();
                    let mut spl = s
                        .char_indices()
                        .auto_escape(|&(_, c)| c == '\\')
                        .splitn(2, |&(esc, (_, c))| !esc && c.is_whitespace(), true)
                        .map(|v: Vec<(bool, (usize, char))>| {
                            let orig_len = v
                                .iter()
                                .map(|&(esc, (_, c))| c.len_utf8() + if esc { 1 } else { 0 })
                                .sum();
                            let s = v
                                .into_iter()
                                .unescape(|&(i, c)| {
                                    if Self::is_invalid_command_char(c) {
                                        None
                                    } else {
                                        Some((i - c.len_utf8(), '\\'))
                                    }
                                })
                                .map(|t| t.1)
                                .collect::<String>();
                            (s, orig_len)
                        });

                    let (mut cmd, orig_cmd_len) = spl.next().unwrap();
                    let sep_len = spl.next().map_or(0, |t: (String, usize)| t.1);
                    let (mut body, orig_body_len) = spl.next().unwrap_or((String::new(), 0));

                    let cmd_span = Span::new(start - 2, orig_cmd_len + sep_len + orig_body_len + 4);
                    let mut body_span = Span::new(start + orig_cmd_len, orig_body_len);

                    match self
                        .commands
                        .get(&cmd)
                        .or_else(|| {
                            if body.is_empty() {
                                let res = self.commands.get("");
                                if res.is_some() {
                                    body = take(&mut cmd);
                                    body_span = Span::new(start, orig_cmd_len);
                                }
                                res
                            } else {
                                None
                            }
                        })
                        .copied()
                    {
                        Some(handler) => {
                            let args = CommandConfig {
                                body,
                                body_span,
                                cmd_span,
                                issues,
                                engine: self.capture(),
                            };
                            res.push(handler(args));
                        }
                        None => {
                            issues.push(Issue {
                                id: "command:unknown",
                                msg: format!(
                                    "invalid or unknown command at {} (starting with `(%{}`)",
                                    RowCol::from_index(start - 2, &orig_s),
                                    if s.len() < 10 { &s } else { &s[..10] }
                                ),
                                span: cmd_span,
                            });
                        }
                    }
                }
            }

            start += s_len;
            if is_command {
                start += 2;
            }
        }

        res.join("")
    }

    /// Creates a new `Vec` to hold issues, before calling [`self.process`](#method.process)
    #[inline]
    pub fn process_new(&mut self, template: String) -> (String, Vec<Issue>) {
        let mut issues = Vec::new();

        let processed = self.process(template, &mut issues);

        (processed, issues)
    }

    // /// processes a string, pushing all encountered issues onto `issues`
    // pub fn process(&mut self, mut s: String, issues: &mut Vec<Issue>) -> String {
    //     let mut changes = vec![];
    //     let mut offs = 0;
    //     println!("process {:?}", s);
    //     while let Some(di) = s[offs..].find('%') {
    //         if s.get(offs + 1..=offs + 1) == Some("%") {
    //             // "%%" is an escaped "%"
    //             offs += 2;
    //             continue;
    //         }
    //         offs += di;
    //         let i = offs + 1;
    //         let sl = &s[i..];
    //
    //         if let Some((len, res, mut new_issues)) = invoke::invoke_block_handlers(sl, i, self) {
    //             // dbgr!(len, &s[offs..offs + len]);
    //             changes.push((offs, len, res));
    //             offs += len;
    //             // dbgr!(&s[offs..], s[offs..].find('%'));
    //             issues.append(&mut new_issues);
    //             continue;
    //         }
    //
    //         if let Some((len, res, mut new_issues)) = invoke::invoke_basic_handlers(sl, i, self) {
    //             // dbgr!(len, &s[offs..offs + len]);
    //             changes.push((offs, len, res));
    //             offs += len;
    //             // dbgr!(&s[offs..], s[offs..].find('%'));
    //             issues.append(&mut new_issues);
    //             continue;
    //         }
    //
    //         issues.push(Issue {
    //             id: "command:unknown",
    //             msg: format!(
    //                 "invalid or unknown command at {} (starting with {}...)",
    //                 RowCol::from_index(offs, &s),
    //                 if s[offs..].len() < 10 {
    //                     &s[offs..]
    //                 } else {
    //                     &s[offs..offs + 10]
    //                 }
    //             ),
    //             span: Span::new(offs, 1),
    //         });
    //
    //         // move one over to find the next `%`
    //         offs += 1;
    //     }
    //     // reverse it to avoid index-shifting
    //     changes.reverse();
    //     // dbgr!();
    //     for (offs, len, res) in changes {
    //         // dbgr!(offs, len, s.len());
    //         s.replace_range(offs..offs + len, &res);
    //     }
    //     println!("done processing. Result: {:?}", s);
    //     if !issues.is_empty() {
    //         println!("issues encountered: {:?}", issues);
    //     }
    //     s
    // }
}

// todo: add more unit tests
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vars() {
        let s = "abc(%var1%)def(%var2%)ghi";
        let mut vars = HashMap::new();
        vars.insert("var1".to_string(), "1".to_string());
        vars.insert("var2".to_string(), "2".to_string());
        let mut en = Engine::with_predefined_commands(vars);
        let (s, i) = en.process_new(s.to_string());
        assert_eq!(i, vec![]);
        assert_eq!(&s, "abc1def2ghi");
    }

    #[test]
    fn test_run() {
        let s = "(%run echo \"test 123\"456 789%)";
        let vars = HashMap::new();
        let mut en = Engine::with_predefined_commands(vars);
        let (s, i) = en.process_new(s.to_string());
        assert_eq!(i, vec![]);
        assert_eq!(&s, "test 123456 789\n");
    }

    #[test]
    fn test_alt() {
        let s = "(%alt :::a%)(%alt ::a:b:c%)";
        let vars = HashMap::new();
        let mut en = Engine::with_predefined_commands(vars);
        let (s, i) = en.process_new(s.to_string());
        assert_eq!(i, vec![]);
        assert_eq!(&s, "aa");
    }

    #[test]
    fn test_lit() {
        let s = "(%lit (%alt abcs%)%)";
        let vars = HashMap::new();
        let mut en = Engine::with_predefined_commands(vars);
        let (s, i) = en.process_new(s.to_string());
        assert_eq!(i, vec![]);
        assert_eq!(&s, "(%alt abcs%)");
    }

    #[test]
    fn test_invalid() {
        let s = "abc(%invalid abc%)lsdfga7vyev(%lit%)sd";
        let vars = HashMap::new();
        let mut en = Engine::with_predefined_commands(vars);
        let (s, i) = en.process_new(s.to_string());
        assert_eq!(
            i,
            vec![Issue {
                id: "command:unknown",
                msg: "invalid or unknown command at 1:4 (starting with `(%invalid ab`)".to_string(),
                span: Span::new(3, 15)
            }]
        );
        assert_eq!(&s, "abclsdfga7vyevsd");
    }

    #[test]
    fn test_unknown_var() {
        let s = "abc(%unknown%)";
        let vars = HashMap::new();
        let mut en = Engine::with_predefined_commands(vars);
        let (s, i) = en.process_new(s.to_string());
        assert_eq!(
            i,
            vec![Issue {
                id: "command:invalid_args",
                msg: "unknown variable: unknown".to_string(),
                span: Span::new(5, 7)
            }]
        );
        assert_eq!(&s, "abc");
    }

    #[test]
    fn test_for_range() {
        let s = "(%for i from 1 to 10;(%i%)%)";
        let vars = HashMap::new();
        let mut en = Engine::with_predefined_commands(vars);
        let (s, i) = en.process_new(s.to_string());
        assert_eq!(i, vec![]);
        assert_eq!(&s, "12345678910");
    }

    #[test]
    fn test_for_in() {
        let s = "(%for i in a:b:c;(%i%)%)";
        let vars = HashMap::new();
        let mut en = Engine::with_predefined_commands(vars);
        let (s, i) = en.process_new(s.to_string());
        assert_eq!(i, vec![]);
        assert_eq!(&s, "abc");
    }

    #[test]
    fn test_for_in_sorted() {
        let s = "(%for i in.sorted((%i%):-)a:b:c;(%i%)%)";
        let vars = HashMap::new();
        let mut en = Engine::with_predefined_commands(vars);
        let (s, i) = en.process_new(s.to_string());
        assert_eq!(i, vec![]);
        assert_eq!(&s, "cba");
    }
}
