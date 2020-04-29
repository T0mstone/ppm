use crate::util::{indicator, AutoEscape, CreateTakeWhileLevelGe0, Span, Unescape};
use crate::{BasicHandler, BlockHandler, Engine, Issue};
use std::collections::HashMap;
use std::iter::once;
use std::mem::replace;

#[inline]
pub fn find_arg_start(s: &str) -> Option<(bool, usize)> {
    // note: if the sequence is invalid, it will search and maybe find the next sequence
    //       however that doesn't matter as no commands may conatin a '%' in their name
    //       and thus it will fail to be recognised
    let mut delay = true;
    s.char_indices()
        // only take up to the first whitespace or to the next percent - those aren't allowed inside command names
        // todo: make this constraint clear in the docs
        .auto_escape(|t| t.1 == '\\')
        .take_while(|&(esc, (_, c))| {
            replace(
                &mut delay,
                esc || (!c.is_whitespace() && c != '%' && c != '('),
            )
        })
        .last()
        .map(|(_, (arg_start, c))| (c == '(', arg_start))
}

pub fn parse_args(s: &str) -> String {
    // note: this code assumes that `s[0] == '('`. Only ever call it that way
    let mut iter = s[1..].chars().auto_escape(indicator('\\')).peekable();
    iter.take_while_lvl_ge0(
        |&(esc, c)| !esc && c == '(',
        |&(esc, c)| !esc && c == ')',
        false,
    )
    .unescape(|_| once('\\'))
    .collect()
}

/// A struct representing the arguments for a basic command
#[derive(Debug, Eq, PartialEq)]
pub struct BasicCommandArgs<'a> {
    /// The region of the whole command (from `%` to the very end)
    pub cmd_span: Span,
    /// The string inside the parentheses (`%cmd` is equivalent to `%cmd()`)
    pub arg_str: String,
    /// All issues that were encountered (will be mutated by handlers)
    pub issues: &'a mut Vec<Issue>,
}

// only returns None if the sequence is invalid
pub fn invoke_basic_handlers(
    s: &str,
    i: usize,
    handlers: &HashMap<String, BasicHandler>,
    engine: &mut Engine,
) -> Option<(usize, String, Vec<Issue>)> {
    // what's going on:
    // `i` is the offset at which `s` starts in the parent string
    //    %abc(def)
    //   i-^  ^[-]
    //        | ^- arg_str.len()
    //  arg_start
    // (keep in mind that this could also be a call without arguments)

    let (head_str, arg_str, delta_len) = match find_arg_start(s) {
        Some((true, head_len)) => (&s[..head_len], parse_args(&s[head_len..]), head_len + 2),
        // note: a call without arguments is equivalent to a call with empty arguments
        Some((false, head_len)) => (&s[..head_len], String::new(), head_len),
        None => (s, String::new(), s.len()),
    };

    // println!("{}", head_str);

    // the *whole* command - including '%' and parentheses
    // even if the '%' is not in `s`
    let cmd_len = 1 + delta_len + arg_str.len();
    let cmd_span = Span::new(i - 1, cmd_len);
    let mut issues = Vec::new();

    let cmd_len = cmd_span.len;

    handlers.get(head_str).map(|handler| {
        let args = BasicCommandArgs {
            cmd_span,
            arg_str,
            issues: &mut issues,
        };
        (cmd_len, handler(args, engine), issues)
    })
}

/// A struct representing the arguments for a block command
#[derive(Debug, Eq, PartialEq)]
pub struct BlockCommandArgs<'a> {
    /// The region of the first half of the command (from `%` to (not including) `%{`)
    pub start_cmd_span: Span,
    /// The region of the whole command (from `%` to the very end)
    pub cmd_span: Span,
    /// The region of the body (the part inside the `%{ %}`)
    pub body_span: Span,
    /// The string inside the parentheses (`%cmd` is equivalent to `%cmd()`)
    pub arg_str: String,
    /// The string inside the body (the part inside the `%{ %}`)
    pub body: String,
    /// All issues that were encountered (will be mutated by handlers)
    pub issues: &'a mut Vec<Issue>,
}

// only returns None if the sequence is invalid
pub fn invoke_block_handlers(
    s: &str,
    i: usize,
    handlers: &HashMap<String, BlockHandler>,
    engine: &mut Engine,
) -> Option<(usize, String, Vec<Issue>)> {
    // what's going on:
    // `i` is the offset at which `s` starts in the parent string
    // all other indices are relative to `s`
    //   %abc(def)
    //  i-^  ^[-]
    //       | ^- args.len()
    //  arg_start
    // (keep in mind that this could also be a call without arguments)

    let (head_str, arg_str, delta_len) = match find_arg_start(s) {
        Some((true, head_len)) => (&s[..head_len], parse_args(&s[head_len..]), head_len + 2),
        // note: a call without arguments is equivalent to a call with empty arguments
        Some((false, head_len)) => (&s[..head_len], String::new(), head_len),
        None => (s, String::new(), s.len()),
    };

    // the `1 +` because this length includes the percent sign
    let start_cmd_len = 1 + delta_len + arg_str.len();
    match s.get(start_cmd_len - 1..start_cmd_len + 1) {
        Some("%{") if s.len() > start_cmd_len + 1 => (),
        _ => return None,
    }
    let tmp = s[start_cmd_len..].chars().collect::<Vec<_>>();
    let body = tmp
        .windows(2)
        .take_while_lvl_ge0(|&sl| sl == &['%', '('], |&sl| sl == &['%', ')'], false)
        .map(|sl| sl[0])
        .collect::<String>();
    // %abc(...)%(defgh%)
    // [-------]  [---]  ^- end
    // ^   ^        ^- body.len()
    // start_cmd_len

    // note: `end` is relative to the percent sign (one before the start of `s`)
    let end = start_cmd_len + body.len() + 4;

    let start_cmd_span = Span::new(i - 1, start_cmd_len);
    let cmd_span = Span::new(i - 1, end);
    let body_span = Span::new(start_cmd_len + 2, body.len());

    let mut issues = Vec::new();

    let cmd_len = cmd_span.len;

    handlers.get(head_str).map(|handler| {
        let args = BlockCommandArgs {
            cmd_span,
            start_cmd_span,
            body_span,
            arg_str,
            body,
            issues: &mut issues,
        };
        (cmd_len, handler(args, engine), issues)
    })
}
