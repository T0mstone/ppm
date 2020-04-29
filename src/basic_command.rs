use crate::util::{indicator, AutoEscape, CreateTakeWhileLevelGe0, Span, Unescape};
use crate::{BasicHandler, Engine, Issue};
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

#[derive(Debug, Eq, PartialEq)]
pub struct BasicCommandArgs<'a> {
    pub cmd_span: Span,
    pub arg_str: String,
    pub issues: &'a mut Vec<Issue>,
}

// only returns None if the sequence is invalid
pub fn invoke_handlers(
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
