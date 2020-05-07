pub use self::for_loop::handler as for_handler;
pub use self::lsdir::handler as lsdir_handler;
#[cfg(feature = "regex")]
pub use self::regex::handler as regex_sub_handler;
use crate::util::{
    head_tail, indicator, indicator_not_escaped, make_absolute, AutoEscape, IterSplit, Span,
    SplitNotEscapedString, Unescape,
};
use crate::{shell_util, CommandConfig, CommandHandler, Issue};
use std::collections::HashMap;
use std::path::Path;

mod for_loop;
mod lsdir;
#[cfg(feature = "regex")]
mod regex;

/// Creates a `HashMap` with all the predefined basic commands.
///
/// Their assigned names are:
/// - `lit` for [`literal_handler`](fn.literal_handler.html)
/// - `eval` for [`eval_handler`](fn.eval_handler.html)
/// - empty for [`get_var_handler`](fn.get_var_handler.html), enabling its use as `(%variable%)`
///     -> this is a special case, for which the engine knwos to view `(%cmd%)`
/// as the command `cmd` if it exists and as the empty command with argument `cmd` otherwise
/// - `var` for [`get_var_handler`](fn.get_var_handler.html)
/// - `let` for [`set_var_handler`](fn.set_var_handler.html)
/// - `run` for [`run_process_handler`](fn.run_process_handler.html)
/// - `alt` for [`fallback_handler`](fn.fallback_handler.html)
/// - `lsdir` for [`lsdir_handler`](fn.lsdir_handler.html)
/// - `re_sub` for [`regex_sub_handler`](fn.regex_sub_handler.html)
/// - `for` for [`for_handler`](fn.for_handler.html)
#[inline]
pub fn get_all_handlers() -> HashMap<String, CommandHandler> {
    let mut res = HashMap::new();
    res.insert("lit".to_string(), literal_handler as _);
    res.insert("eval".to_string(), eval_handler as _);
    res.insert("".to_string(), get_var_handler as _);
    res.insert("var".to_string(), get_var_handler as _);
    res.insert("let".to_string(), set_var_handler as _);
    res.insert("run".to_string(), run_process_handler as _);
    res.insert("alt".to_string(), fallback_handler as _);
    res.insert("lsdir".to_string(), lsdir_handler as _);
    #[cfg(feature = "regex")]
    res.insert("re_sub".to_string(), regex_sub_handler as _);
    res.insert("for".to_string(), for_handler as _);
    res
}

/// calls `engine.process` on the argument
#[inline]
pub fn eval_handler(mut cfg: CommandConfig) -> String {
    cfg.process_body()
}

/// outputs its argument literally, preventing it from being processed
#[inline]
pub fn literal_handler(args: CommandConfig) -> String {
    args.body
}

/// substitutes calls to variables stored in `engine.vars` with their value
/// - argument: anything - the variable name
/// - does not call `engine.process` on its argument before processing and on the final value
pub fn get_var_handler(mut cfg: CommandConfig) -> String {
    let err = cfg.invalid_args(format!("unknown variable: {}", cfg.body));
    let key = cfg.process_body();
    let issues = cfg.issues;
    cfg.engine.vars.get(&key).cloned().unwrap_or_else(|| {
        issues.push(err);
        String::new()
    })
}

/// sets a variable
/// - arguments: the name of the variable. The block is the value the variable is set to
/// - calls `engine.process` on both its arguments before evaluating them
pub fn set_var_handler(mut cfg: CommandConfig) -> String {
    let mut spl = cfg.body.split_not_escaped('=', '\\', false).into_iter();

    let var = spl.next().unwrap();
    let val = spl.next().unwrap_or_default();

    let var = cfg.process(var);
    let val = cfg.process(val);

    cfg.engine.vars.insert(var, val);
    String::new()
}

/// runs a process based on the argument
/// - argument: a basic shell-like syntax for spawning a process (supports string literals for escaping spaces)
/// - calls `engine.process` on each argument before the process is run and on the process' output
pub fn run_process_handler(mut cfg: CommandConfig) -> String {
    let v = shell_util::split_args(&cfg.body)
        .into_iter()
        .map(|s| cfg.process(s))
        .collect();
    // the `split_args` will always at least produce an empty string for `cmd`
    let (cmd, argv) = head_tail(v).unwrap();

    if cmd.is_empty() {
        cfg.push_missing_args("no process to run given");
        return String::new();
    }

    let output = match cfg.engine.root_path.clone() {
        Some(cwd) => std::process::Command::new(cmd)
            .args(argv)
            .current_dir(&cwd)
            .output(),
        None => std::process::Command::new(cmd).args(argv).output(),
    };

    let output = match output {
        Ok(x) => x,
        Err(e) => {
            cfg.issues.push(Issue::io_error(
                e,
                cfg.cmd_span,
                Some("while trying to run process"),
            ));
            return String::new();
        }
    };

    String::from_utf8_lossy(&output.stdout).to_string()
}

/// outputs the first of its arguments that is not empty
/// - arguments: separated by colons
///     - escaping colons with `'\\'` is supported, all other instances of `'\\'` are left unchanged
/// - short circuits
/// - calls `engine.process` on every argument before deciding
pub fn fallback_handler(mut cfg: CommandConfig) -> String {
    let spl = cfg
        .body
        .chars()
        .auto_escape(indicator('\\'))
        .split(indicator_not_escaped(':'), false)
        .collect::<Vec<_>>();

    let mut cumulen = None;
    for v in spl {
        let len = v
            .iter()
            .map(|&(esc, c)| c.len_utf8() + if esc { 1 } else { 0 })
            .sum();
        let subspan = Span::new(cumulen.unwrap_or(0), len);
        match cumulen.as_mut() {
            Some(cumulen) => *cumulen += len + 1,
            None => cumulen = Some(len),
        }

        let s = v
            .into_iter()
            .unescape(|&c| if c == ':' { vec![] } else { vec!['\\'] })
            .collect();

        let s = cfg.process_subbody(s, subspan).unwrap();

        if !s.is_empty() {
            return s;
        }
    }

    // all of the args were empty - this is not an issue
    String::new()
}

/// includes another file inside a file
/// - argument: the path to the file
/// - calls `engine.process` on the argument before processing it
/// - does not call `engine.process` on the file before inserting it
pub fn include_handler(mut cfg: CommandConfig) -> String {
    let arg = cfg.process_body();
    let path = match make_absolute(Path::new(&arg), cfg.engine.root_path.clone()) {
        Ok(x) => x,
        Err(e) => {
            cfg.issues.push(Issue::io_error(
                e,
                cfg.cmd_span,
                Some("while trying to get the current directory"),
            ));
            return String::new();
        }
    };

    match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            cfg.issues.push(Issue::io_error(
                e,
                cfg.cmd_span,
                Some("while trying to read file"),
            ));
            String::new()
        }
    }
}

// todo: macros
