pub use self::for_loop::handler as for_handler;
pub use self::lsdir::handler as lsdir_handler;
#[cfg(feature = "regex")]
pub use self::regex::handler as regex_sub_handler;
use crate::util::{head_tail, make_absolute, SplitNotEscapedString};
use crate::{shell_util, CommandConfig, CommandHandler, Issue};
use std::collections::HashMap;
use std::path::Path;

// overall what-to-process guide:
// arguments should be processed unless they have special syntax
// return values should only be processed if they can be directly influenced by the user

/// Some tools to ease creating commands.
pub mod tools;

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
/// - `sort_by` for [`sort_by_handler`](fn.sort_handler.html)
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
    res.insert("sort_by".to_string(), sort_by_handler as _);
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
pub fn literal_handler(cfg: CommandConfig) -> String {
    cfg.body
}

/// substitutes calls to variables stored in `engine.vars` with their value
/// - argument: anything - the variable name
/// - does not call `engine.process` on its argument before processing and on the final value
pub fn get_var_handler(mut cfg: CommandConfig) -> String {
    let key = cfg.process_body();
    let err = cfg.invalid_args(format!("unknown variable: {}", key));
    let issues = cfg.issues;
    cfg.engine.vars.get(&key).cloned().unwrap_or_else(|| {
        issues.push(err);
        String::new()
    })
}

/// sets a variable
/// - arguments: the name of the variable. The block is the value the variable is set to
/// - calls `engine.process` on its argument string before doing anything
pub fn set_var_handler(mut cfg: CommandConfig) -> String {
    let body = cfg.process_body();

    let mut spl = body
        .split_not_escaped::<Vec<_>>('=', '\\', false)
        .into_iter();

    let var = spl.next().unwrap();
    let val = spl.next().unwrap_or_default();

    cfg.engine.vars.insert(var, val);
    String::new()
}

/// runs a process based on the argument
/// - argument: a basic shell-like syntax for spawning a process (supports string literals for escaping spaces)
/// - calls `engine.process` on its argument string before doing anything
pub fn run_process_handler(mut cfg: CommandConfig) -> String {
    let v = shell_util::split_args(&cfg.process_body());
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
/// - arguments: separated by colons (using [`split_args`](tools/split_args.html))
///     - escaping colons with `'\\'` is supported, all other instances of `'\\'` are left unchanged
/// - short circuits
/// - calls `engine.process` on its argument string before doing anything
pub fn fallback_handler(mut cfg: CommandConfig) -> String {
    // let spl = cfg
    //     .process_body()
    //     .chars()
    //     .auto_escape(indicator('\\'))
    //     .split::<_, Vec<_>>(indicator_not_escaped(':'), false)
    //     .collect::<Vec<_>>();

    // because it is already processed, we don't need tools::split_args here
    let spl: Vec<String> = cfg.process_body().split_not_escaped(':', '\\', false);

    // let mut cumulen = None;
    for s in spl {
        // let len = v
        //     .iter()
        //     .map(|&(esc, c)| c.len_utf8() + if esc { 1 } else { 0 })
        //     .sum();
        // let subspan = Span::new(cumulen.unwrap_or(0), len);
        // match cumulen.as_mut() {
        //     Some(cumulen) => *cumulen += len + 1,
        //     None => cumulen = Some(len),
        // }

        // let s = v
        //     .into_iter()
        //     .unescape(|&c| if c == ':' { None } else { Some('\\') })
        //     .collect::<String>();

        if !s.is_empty() {
            return s;
        }
    }

    // all of the args were empty - this is not an issue
    String::new()
}

/// includes another file inside a file
/// - argument: the path to the file
/// - calls `engine.process` on its argument string before doing anything
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

#[inline]
fn join_reescape_colon(i: impl Iterator<Item = String>) -> String {
    i.map(|s| {
        s.chars()
            .flat_map(|c| {
                if c == ':' { Some('\\') } else { None }
                    .into_iter()
                    .chain(std::iter::once(c))
            })
            .collect::<String>()
    })
    .collect::<Vec<_>>()
    .join(":")
}
// map is basically identical to for - why did i make a different function ofr it???
// pub fn map_handler(mut cfg: CommandConfig) -> String {
//     let body = cfg.process_body();
//     let mut args = body.split_not_escaped(':', '\\', false);
//     let first_arg = args.remove(0);
//     let mut spl = first_arg
//         .splitn_not_escaped(2, ' ', '\\', false)
//         .into_iter();
//     let var = spl.next().unwrap();
//     let expr = match spl.next() {
//         Some(x) => x,
//         None => {
//             cfg.push_invalid_args("no map expression provided".to_string());
//             return join_re_escape_colon(args.into_iter());
//         }
//     };
//     let orig_var = cfg.engine.vars.remove(&var);
//     let res = join_re_escape_colon(args.into_iter().map(|s| {
//         cfg.engine.vars.insert(var.clone(), s);
//         cfg.process(expr.to_string())
//     }));
//     if let Some(s) = orig_var {
//         cfg.engine.vars.insert(var.to_string(), s);
//     }
//     res
// }

// /// Sorts a `:`-separated list
// /// - calls `engine.process` on the entire argument before doing anything
// /// - the first argument specifies the sorting order
// ///     - valid values are: `+`, `asc`, `ascending`, `inc`, `increasing`, `-`, `desc`, `descending`, `dec`, `decreasing`
// /// - outputs a `:`-separated list, just sorted
// pub fn sort_handler(mut cfg: CommandConfig) -> String {
//     let body = cfg.process_body();
//     let mut args = body.split_not_escaped(':', '\\', false);
//     let first_arg = args.remove(0);
//     let desc = match first_arg.as_str() {
//         "+" | "asc" | "ascending" | "inc" | "increasing" => false,
//         "-" | "desc" | "descending" | "dec" | "decreasing" => true,
//         s => {
//             cfg.push_invalid_args(format!("invalid sorting order: {}", s));
//             return join_reescape_colon(args.into_iter());
//         }
//     };
//     args.sort();
//     if desc {
//         args.reverse();
//     }
//     join_reescape_colon(args.into_iter())
// }

/// Sorts a `:`-separated list, according to a key
/// - arguments: separated by colons (using [`split_args`](tools/split_args.html))
/// - calls `engine.process` on the entire second argument before doing anything
/// - the first argument follows the syntax `<variable> <order> <expr>`
///     - `<variable>` is the variable by which the current element can be referenced in `<expr>`
///     - `<order>` is one of `+`, `asc`, `ascending`, `inc`, `increasing`, `-`, `desc`, `descending`, `dec`, `decreasing`
///     - `<expr>` is the expression by which the entries will be compared
/// - outputs a sorted `:`-separated list
pub fn sort_by_handler(mut cfg: CommandConfig) -> String {
    // let mut args = cfg
    //     .body
    //     .splitn_not_escaped::<Vec<_>>(2, ':', '\\', false)
    //     .into_iter();

    // note: a part of the first argument has access to the loop variable, thus it can't be processed beforehand
    let mut args = tools::splitn_args(2, cfg.body.clone()).into_iter();
    let first_arg = args.next().unwrap();
    let mut spl = first_arg
        .splitn_not_escaped::<Vec<_>>(3, ' ', '\\', false)
        .into_iter();
    let var = spl.next().unwrap();
    let desc = match spl.next().as_deref() {
        Some(s) => match s {
            "+" | "asc" | "ascending" | "inc" | "increasing" => false,
            "-" | "desc" | "descending" | "dec" | "decreasing" => true,
            _ => {
                cfg.push_invalid_args(format!("invalid sorting order: {}", s));
                return args.next().unwrap_or_default();
            }
        },
        None => {
            cfg.push_invalid_args("expected sorting order, got end of argument".to_string());
            return args.next().unwrap_or_default();
        }
    };
    let expr = match spl.next() {
        Some(x) => x,
        None => {
            cfg.push_invalid_args("no map expression provided".to_string());
            return args.next().unwrap_or_default();
        }
    };

    let args = match args.next() {
        Some(x) => x,
        None => {
            cfg.push_invalid_args("no list to sort provided".to_string());
            return String::new();
        }
    };
    // because it is already processed, we don't need tools::split_args here
    let mut args = cfg
        .process(args)
        .split_not_escaped::<Vec<_>>(':', '\\', false);

    let orig_var = cfg.engine.vars.remove(&var);
    args.sort_by_key(|s| {
        cfg.engine.vars.insert(var.clone(), s.clone());
        cfg.process(expr.to_string())
    });
    if desc {
        args.reverse();
    }
    if let Some(s) = orig_var {
        cfg.engine.vars.insert(var, s);
    }
    join_reescape_colon(args.into_iter())
}

// maybe_todo: macros
