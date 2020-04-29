use crate::util::{CreateTakeWhileLevelGe0, Span};
use crate::{basic_command, Engine};
use crate::{BlockHandler, Issue};
use std::collections::HashMap;

#[derive(Debug, Eq, PartialEq)]
pub struct BlockCommandArgs<'a> {
    pub start_cmd_span: Span,
    pub cmd_span: Span,
    pub body_span: Span,
    pub arg_str: String,
    pub body: String,
    pub issues: &'a mut Vec<Issue>,
}

impl<'a> BlockCommandArgs<'a> {
    #[inline]
    pub fn push_issue(&mut self, issue: Issue) {
        self.issues.push(issue);
    }
}

// only returns None if the sequence is invalid
pub fn invoke_handlers(
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

    let (head_str, arg_str, delta_len) = match basic_command::find_arg_start(s) {
        Some((true, head_len)) => (
            &s[..head_len],
            basic_command::parse_args(&s[head_len..]),
            head_len + 2,
        ),
        // note: a call without arguments is equivalent to a call with empty arguments
        Some((false, head_len)) => (&s[..head_len], String::new(), head_len),
        None => (s, String::new(), s.len()),
    };

    // this length includes the percent sign and is thus one greater than you'd expect
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
