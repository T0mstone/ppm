use crate::invoke::BlockCommandArgs;
use crate::util::{AutoEscape, CreateTakeWhileLevelGe0, SplitNotEscapedString, Unescape};
use crate::{Engine, Issue};
use std::iter::once;
use tlib::iter_tools::{indicator, indicator_not_escaped, unescape_all};

#[derive(Debug, Clone, Eq, PartialEq)]
enum ForConfig {
    List(Vec<String>, Option<String>, bool),
    Range(i128, i128),
}

impl ForConfig {
    pub fn new(args: &BlockCommandArgs) -> Result<(String, Self), Issue> {
        let mut spl = args.arg_str.split(' ');
        let loopvar = spl
            .next()
            .ok_or(args.invalid_args("no loop variable given".to_string()))?
            .to_string();
        match spl.next() {
            Some(s) if s.starts_with("in.sorted(") => {
                const LEN: usize = "in.sorted(".len();
                let full: String = once(s).chain(spl).collect::<Vec<_>>().join(" ");

                let s2 = full[LEN..]
                    .chars()
                    .auto_escape(indicator('\\'))
                    .take_while_lvl_ge0(
                        indicator_not_escaped('('),
                        indicator_not_escaped(')'),
                        false,
                    )
                    .unescape(unescape_all('\\'))
                    .collect::<String>();

                let mut spl = s2.splitn_not_escaped(2, ':', '\\', false).into_iter();

                let key = spl.next().unwrap();
                let desc = match spl.next() {
                    Some(s) => match &s[..] {
                        "asc" | "ascending" | "inc" | "increasing" | "+" => false,
                        "desc" | "descending" | "dec" | "decreasing" | "-" => true,
                        _ => {
                            return Err(Issue {
                                id: "command:invalid_args:partial",
                                msg: format!("unknown sorting order: {}", s),
                                span: args.start_cmd_span,
                            });
                        }
                    },
                    None => false,
                };

                let arg = if s2.len() == LEN + full.len() {
                    args.invalid_args("no list to iterate over given".to_string());
                    String::new()
                } else {
                    full[s2.len() + 1..].to_string()
                };

                Ok((
                    loopvar,
                    ForConfig::List(arg.split_not_escaped(':', '\\', false), Some(key), desc),
                ))
            }
            Some("in") => {
                let arg = spl.collect::<Vec<_>>().join(" ");
                Ok((
                    loopvar,
                    ForConfig::List(arg.split_not_escaped(':', '\\', false), None, false),
                ))
            }
            Some("from") => {
                let from = match spl.next().map(|s| s.parse::<i128>().map_err(|_| s)) {
                    Some(Ok(x)) => x,
                    Some(Err(s)) => {
                        return Err(args.invalid_args(format!("invalid starting integer: {}", s)))
                    }
                    None => return Err(args.invalid_args("no starting integer given".to_string())),
                };
                match spl.next() {
                    Some("to") => (),
                    Some(s) => return Err(args.invalid_args(format!("invalid range end: {}", s))),
                    None => return Err(args.invalid_args("no range end given".to_string())),
                }
                let to = match spl.next().map(|s| s.parse::<i128>().map_err(|_| s)) {
                    Some(Ok(x)) => x,
                    Some(Err(s)) => {
                        return Err(args.invalid_args(format!("invalid ending integer: {}", s)))
                    }
                    None => return Err(args.invalid_args("no ending integer given".to_string())),
                };
                Ok((loopvar, ForConfig::Range(from, to)))
            }
            Some(x) => Err(args.invalid_args(format!("unknown repeat kind: {}", x))),
            None => Err(args.missing_args("no repeat kind given")),
        }
    }
}

/// a for-loop that repeats its body and updates a loop variable according to the argument
/// - arguments: a loop variable (any string) and then a loop method, separated by a space (`' '`)
///     - escaping a space with `'\\'` is supported, all other instances of `'\\'` are left unchanged
///     - there are two loop methods:
///         - `from _ to _` where the `_` are integers
///             - if the first number is larger than the second, it does nothing
///         - `in _` where the `_` is a list (colon-delimeted)
///             - escaping colons with `'\\'` is supported, all other instances of `'\\'` are left unchanged
///         - `in.sorted(key:order) _` is like `in _` but the `key` and `order` are responsible for sorting the list before iterating
///             - `key` is a string that will be interpreted by the engine and sorted by (has access to the loop variable)
///             - `order` is one of `asc, ascending, inc, increasing, +` (going from low to high) or `desc, descending, dec, decreasing, -` (going from high to low)
/// - calls `engine.process` each time with the loop variable added to the variables
///     - it overwrites any previous value that name had, but restores it once finished
pub fn handler(args: BlockCommandArgs, engine: &mut Engine) -> String {
    let (loopvar, config) = match ForConfig::new(&args) {
        Ok(x) => x,
        Err(e) => {
            args.issues.push(e);
            return String::new();
        }
    };

    let prev_loopvar_val = engine.vars.remove(&loopvar);

    let mut res = Vec::new();
    match config {
        ForConfig::Range(a, b) => {
            if a > b {
                return String::new();
            }
            for i in a..=b {
                engine.vars.insert(loopvar.clone(), i.to_string());
                res.push(engine.process(args.body.clone(), args.issues));
            }
        }
        ForConfig::List(mut v, key, desc) => {
            if let Some(k) = key {
                v.sort_by_key(|s| {
                    engine.vars.insert(loopvar.clone(), s.clone());
                    engine.process(k.clone(), args.issues)
                });
            }
            if desc {
                v.reverse();
            }
            for s in &v {
                engine.vars.insert(loopvar.clone(), s.clone());
                res.push(engine.process(args.body.clone(), args.issues));
            }
        }
    }
    match prev_loopvar_val {
        Some(x) => engine.vars.insert(loopvar, x),
        None => engine.vars.remove(&loopvar),
    };
    res.join("")
}
