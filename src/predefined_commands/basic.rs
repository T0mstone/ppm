pub use super::lsdir::handler as lsdir_handler;
#[cfg(feature = "regex")]
pub use super::regex::regex_sub_basic as regex_sub_handler;
use crate::invoke::BasicCommandArgs;
use crate::util::{
    head_tail, indicator, indicator_not_escaped, make_absolute, AutoEscape, IterSplit, Span,
    Unescape,
};
use crate::{absorb_new_issues, shell_util, BasicHandler};
use crate::{Engine, Issue};
use std::collections::HashMap;
use std::path::Path;

#[inline]
/// Creates a `HashMap` with all the predefined basic commands.
///
/// Their assigned names are:
/// - empty for [`var_handler`](fn.var_handler.html), enabling its use as `%(variable)`
/// - `run` for [`run_process_handler`](fn.run_process_handler.html)
/// - `lit` for [`literal_handler`](fn.literal_handler.html)
/// - `alt` for [`fallback_handler`](fn.fallback_handler.html)
/// - `lsdir` for [`lsdir_handler`](fn.lsdir_handler.html)
/// - `re_isub` for [`regex_sub_handler`](fn.regex_sub_handler.html)
pub fn get_all_handlers() -> HashMap<String, BasicHandler> {
    let mut res = HashMap::new();
    res.insert("".to_string(), var_handler as _);
    res.insert("run".to_string(), run_process_handler as _);
    res.insert("lit".to_string(), literal_handler as _);
    res.insert("alt".to_string(), fallback_handler as _);
    res.insert("lsdir".to_string(), lsdir_handler as _);
    // 'i' means inline
    #[cfg(feature = "regex")]
    res.insert("re_isub".to_string(), regex_sub_handler as _);
    res
}

/// substitutes calls to variables stored in `engine.vars` with their value
/// - argument: anything - the variable name
/// - calls `engine.process` on its argument before processing and on the final value
pub fn var_handler(args: BasicCommandArgs, engine: &mut Engine) -> String {
    let key = engine.process(args.arg_str.clone(), args.issues);
    engine.process(
        engine.vars.get(&key).cloned().unwrap_or_else(|| {
            args.issues.push(Issue {
                id: "command:invalid_args",
                msg: format!("unknown variable: {}", args.arg_str),
                span: args.cmd_span,
            });
            String::new()
        }),
        args.issues,
    )
}

/// runs a process based on the argument
/// - argument: a basic shell-like syntax for spawning a process (supports string literals for escaping spaces)
/// - calls `engine.process` on each argument before the process is run and on the process' output
pub fn run_process_handler(args: BasicCommandArgs, engine: &mut Engine) -> String {
    let v = shell_util::split_args(&args.arg_str)
        .into_iter()
        .map(|s| engine.process(s, args.issues))
        .collect();
    // the `split_args` will always at least produce an empty string for `cmd`
    let (cmd, argv) = head_tail(v).unwrap();

    if cmd.is_empty() {
        args.issues.push(Issue {
            id: "command:missing_args",
            msg: "no process to run given".to_string(),
            span: args.cmd_span,
        });
        return String::new();
    }

    let output = match engine.root_path.clone() {
        Some(cwd) => std::process::Command::new(cmd)
            .args(argv)
            .current_dir(&cwd)
            .output(),
        None => std::process::Command::new(cmd).args(argv).output(),
    };

    let output = match output {
        Ok(x) => x,
        Err(e) => {
            args.issues.push(Issue::io_error(
                e,
                args.cmd_span,
                Some("while trying to run process"),
            ));
            return String::new();
        }
    };

    String::from_utf8_lossy(&output.stdout).to_string()
}

/// outputs its argument literally, preventing it from being processed
#[inline]
pub fn literal_handler(args: BasicCommandArgs, _: &mut Engine) -> String {
    args.arg_str
}

/// outputs the first of its arguments that is not empty
/// - arguments: separated by colons
///     - escaping colons with `'\\'` is supported, all other instances of `'\\'` are left unchanged
/// - short circuits
/// - calls `engine.process` on every argument before deciding
pub fn fallback_handler(args: BasicCommandArgs, engine: &mut Engine) -> String {
    let mut spl = args
        .arg_str
        .chars()
        .auto_escape(indicator('\\'))
        .split(indicator_not_escaped(':'), false);

    let mut cumulen = None;
    while let Some(v) = spl.next() {
        let len = v
            .iter()
            .map(|&(esc, c)| c.len_utf8() + if esc { 1 } else { 0 })
            .sum();
        let subspan = Span::new(args.cmd_span.start + cumulen.unwrap_or(0), len);
        match cumulen.as_mut() {
            Some(cumulen) => *cumulen += len + 1,
            None => cumulen = Some(len),
        }

        let s = v
            .into_iter()
            .unescape(|&c| if c == ':' { vec![] } else { vec!['\\'] })
            .collect();

        let mut new_issues = Vec::new();
        let s = engine.process(s, &mut new_issues);
        absorb_new_issues(args.issues, subspan, new_issues);

        if !s.is_empty() {
            return s;
        }
    }

    // all of the args were empty - this is not an issue
    String::new()
}

/// includes another file inside a file
/// - argument: the path to the file
/// - calls `engine.process` on the file before inserting it
pub fn include_handler(args: BasicCommandArgs, engine: &mut Engine) -> String {
    let path = match make_absolute(Path::new(&args.arg_str), engine.root_path.clone()) {
        Ok(x) => x,
        Err(e) => {
            args.issues.push(Issue::io_error(
                e,
                args.cmd_span,
                Some("while trying to get the current directory"),
            ));
            return String::new();
        }
    };

    match std::fs::read_to_string(path) {
        Ok(s) => engine.process(s, args.issues),
        Err(e) => {
            args.issues.push(Issue::io_error(
                e,
                args.cmd_span,
                Some("while trying to read file"),
            ));
            String::new()
        }
    }
}

/// includes another file inside a file
/// - argument: the path to the file
/// - does not call `engine.process` on the file before inserting it
pub fn include_literal_handler(args: BasicCommandArgs, engine: &mut Engine) -> String {
    let path = match make_absolute(Path::new(&args.arg_str), engine.root_path.clone()) {
        Ok(x) => x,
        Err(e) => {
            args.issues.push(Issue::io_error(
                e,
                args.cmd_span,
                Some("while trying to get the current directory"),
            ));
            return String::new();
        }
    };

    match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            args.issues.push(Issue::io_error(
                e,
                args.cmd_span,
                Some("while trying to read file"),
            ));
            String::new()
        }
    }
}
