use crate::invoke::BasicCommandArgs;
use crate::shell_util::matches_pattern;
use crate::util::{make_absolute, SplitNotEscapedString};
use crate::{Engine, Issue};
use std::path::Path;

#[derive(Default, Debug, Clone, Eq, PartialEq)]
pub struct LsdirConfig {
    pub path: String,
    pub exclude_by_name: Vec<String>,
    pub include_only_by_name: Option<Vec<String>>,
    pub sort_key: Option<String>,
    pub sort_descending: bool,
}

impl LsdirConfig {
    pub fn new(args: &mut BasicCommandArgs) -> Result<Self, Issue> {
        let mut res = Self::default();

        let mut spl = args.arg_str.split_not_escaped(':', '\\', false);
        if spl.is_empty() {
            return Err(Issue {
                id: "command:missing_args",
                msg: "no path given".to_string(),
                span: args.cmd_span,
            });
        }

        res.path = spl.remove(0);

        for arg in spl {
            let mut spl = arg.splitn(2, ' ');
            let verb = spl.next().unwrap();
            let object = match spl.next() {
                Some(x) => x,
                None => continue,
            };
            match verb {
                "sort_key" => res.sort_key = Some(object.to_string()),
                "sort_order" => {
                    if ["-", "desc", "descending", "decreasing", "dec"].contains(&object) {
                        res.sort_descending = true;
                    } else if !["+", "asc", "ascemdomg", "increasing", "inc"].contains(&object) {
                        args.issues.push(Issue {
                            id: "command:invalid_args:partial",
                            msg: format!(
                                "warning: unknown sort_order: `{}`. Try `+` or `-`",
                                object
                            ),
                            span: args.cmd_span,
                        });
                    }
                }
                "exclude_names" => {
                    res.exclude_by_name
                        .append(&mut object.split_not_escaped(' ', '\\', false));
                }
                "include_only_names" => {
                    if res.include_only_by_name.is_none() {
                        res.include_only_by_name = Some(Vec::new());
                    }
                    res.include_only_by_name
                        .as_mut()
                        .unwrap()
                        .append(&mut object.split_not_escaped(' ', '\\', false));
                }
                verb => {
                    args.issues.push(Issue {
                        id: "command:invalid_args:partial",
                        msg: format!("warning: ignoring unrecognised verb `{}`", verb),
                        span: args.cmd_span,
                    });
                }
            }
        }

        Ok(res)
    }

    pub fn is_included<P: AsRef<Path>>(&self, p: P) -> bool {
        p.as_ref().file_name().map_or(true, |name| {
            let tmp = name.to_string_lossy();
            let name_str = tmp.as_ref();
            self.include_only_by_name
                .as_ref()
                .map_or(true, |v| v.iter().any(|s| matches_pattern(name_str, s)))
                && self
                    .exclude_by_name
                    .iter()
                    .all(|s| !matches_pattern(name_str, s))
        })
    }
}

/// outputs a filtered list of entries (as full paths) in the directory provided as argument
/// - arguments: separated by colons
///     - escaping colons with `'\\'` is supported, all other instances of `'\\'` are left unchanged
///     - first argument: the directory to take entries from
///     - remaining arguments: each has the form `<verb> <object>`.
///         -  escaping `' '` with `'\\'` is supported, all other instances of `'\\'` are left unchanged. Possible verbs are:
///         - `sort_key`: defines a string that is interpolated for each entry, according to which the entries are sorted
///         - `sort_order`: defines an order (increasing or decreasing) to which the entries are sorted
///         - `exclude_names`: the object is a whitespace-separated list of patterns (`'\ '` to escape a whitespace). Files whose names match one of these patterns will not be listed
///             - the patterns support character-by-character equality as well as single-star-globs
///         - `include_only_names`: the object is a whitespace-separated list of patterns (`'\ '` to escape a whitespace). Only Files whose names match one of these patterns will be listed
pub fn handler(mut args: BasicCommandArgs, engine: &mut Engine) -> String {
    let config = match LsdirConfig::new(&mut args) {
        Ok(x) => x,
        Err(e) => {
            args.issues.push(e);
            return String::new();
        }
    };

    let path: &Path = config.path.as_ref();
    let dir = match make_absolute(path, engine.root_path.clone()) {
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

    let iter = match std::fs::read_dir(&dir) {
        Ok(x) => x,
        Err(e) => {
            args.issues.push(Issue::io_error(
                e,
                args.cmd_span,
                Some(&format!(
                    "while trying to read the directory {}",
                    dir.display()
                )),
            ));
            return String::new();
        }
    };

    iter.filter_map(|r| match r {
        Ok(x) => Some(x),
        Err(e) => {
            args.issues.push(Issue::io_error(
                e,
                args.cmd_span,
                Some(&format!(
                    "while reading a file from directory {}",
                    dir.display()
                )),
            ));
            None
        }
    })
    .filter(|d| config.is_included(d.path()))
    .map(|d| {
        d.path()
            .to_string_lossy()
            .replace('\\', "\\\\")
            .replace(':', "\\:")
    })
    .collect::<Vec<_>>()
    .join(":")
}
