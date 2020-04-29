use crate::block_command::BlockCommandArgs;
use crate::shell_util::matches_pattern;
use crate::util::{
    indicator, unescape_all_except, unescaped_indicator, AutoEscape, IterSplit, SplitUnescString,
    Unescape,
};
use crate::{absorb_new_issues, Engine, Issue};
use std::path::Path;

#[derive(Default, Debug, Clone, Eq, PartialEq)]
pub struct ForFilesSettings {
    pub var: Option<String>,
    pub path: String,
    pub exclude_by_name: Vec<String>,
    pub include_only_by_name: Option<Vec<String>>,
    pub sort_key: Option<String>,
    pub sort_descending: bool,
}

impl ForFilesSettings {
    pub fn new(a: &mut BlockCommandArgs) -> Result<Self, Issue> {
        let mut res = ForFilesSettings::default();
        let spl = a
            .arg_str
            .chars()
            .auto_escape(indicator('\\'))
            .split(unescaped_indicator(':'), false)
            .map(|v| {
                v.into_iter()
                    // .unescape(|&c: &char| if c == ':' { vec![] } else { vec!['\\'] })
                    .unescape(unescape_all_except(':', '\\'))
                    .collect::<String>()
            })
            .collect::<Vec<_>>();
        let mut path = None;

        for arg in spl {
            let mut spl = arg.splitn(2, ' ');
            let verb = spl.next().unwrap();
            let object = match spl.next() {
                Some(x) => x,
                None => continue,
            };
            match verb {
                "with" => res.var = Some(object.to_string()),
                "in" => path = Some(object.to_string()),
                "sort_key" => res.sort_key = Some(object.to_string()),
                "sort_order" => {
                    if ["-", "desc", "descending", "decreasing", "dec"].contains(&object) {
                        res.sort_descending = true;
                    } else if !["+", "asc", "ascemdomg", "increasing", "inc"].contains(&object) {
                        a.push_issue(Issue {
                            id: "command:specific:forfiles:unknown_sort_order",
                            msg: format!(
                                "warning: unknown sort_order: `{}`. Try `+` or `-`",
                                object
                            ),
                            span: a.start_cmd_span,
                        });
                    }
                }
                "exclude_names" => {
                    res.exclude_by_name
                        .append(&mut object.split_unescaped(' ', '\\', false));
                }
                "include_only_names" => {
                    if res.include_only_by_name.is_none() {
                        res.include_only_by_name = Some(Vec::new());
                    }
                    res.include_only_by_name
                        .as_mut()
                        .unwrap()
                        .append(&mut object.split_unescaped(' ', '\\', false));
                }
                verb => {
                    a.push_issue(Issue {
                        id: "command:specific:forfiles:unknown_verb",
                        msg: format!("warning: ignoring unrecognised verb `{}`", verb),
                        span: a.start_cmd_span,
                    });
                }
            }
        }

        res.path = path.ok_or(Issue {
            id: "command:invalid_args:forfiles",
            msg: "no path given".to_string(),
            span: a.start_cmd_span,
        })?;
        Ok(res)
    }

    pub fn is_excluded<P: AsRef<Path>>(&self, p: P) -> bool {
        p.as_ref().file_name().map_or(true, |name| {
            let tmp = name.to_string_lossy();
            let name_str = tmp.as_ref();
            self.exclude_by_name
                .iter()
                .any(|s| matches_pattern(name_str, s))
                || self
                    .include_only_by_name
                    .as_ref()
                    .map_or(false, |v| v.iter().all(|s| !matches_pattern(name_str, s)))
        })
    }
}

pub fn handler(mut a: BlockCommandArgs, settings: ForFilesSettings, engine: &Engine) -> String {
    let path: &Path = settings.path.as_ref();
    let dir = if path.is_relative() {
        let mut dir = match engine.root_path.clone() {
            Some(x) => x,
            None => match std::env::current_dir() {
                Ok(x) => x,
                Err(e) => {
                    a.issues.push(Issue::io_error(
                        e,
                        a.cmd_span,
                        Some("while trying to get current directory"),
                    ));
                    return String::new();
                }
            },
        };
        dir.push(path);
        dir
    } else {
        path.to_path_buf()
    };

    let mut res = vec![];

    let iter = match std::fs::read_dir(&dir) {
        Ok(x) => x,
        Err(e) => {
            a.push_issue(Issue::io_error(
                e,
                a.cmd_span,
                Some(&format!(
                    "while trying to read the directory {}",
                    dir.display()
                )),
            ));
            return String::new();
        }
    };

    for entry in iter {
        match entry {
            Ok(e) => {
                if settings.is_excluded(e.path()) {
                    // file is excluded or not included
                    continue;
                }
                let mut eng = engine.clone();
                if let Some(var) = settings.var.as_ref() {
                    if let Some(filename) = e.path().file_name() {
                        eng.vars
                            .insert(var.to_string(), filename.to_string_lossy().to_string());
                    }
                }
                eng.vars.extend((eng.get_extra_vars)(e.path()));
                let (body, new_issues) = eng.process(a.body.clone());
                let mut span = a.body_span;
                // we don't need the percent sign
                span.shift_start_forward(1);
                absorb_new_issues(&mut a.issues, span, new_issues);
                res.push((eng, body));
            }
            Err(e) => {
                a.push_issue(Issue::io_error(
                    e,
                    a.cmd_span,
                    Some(&format!(
                        "while reading a file from directory {}",
                        dir.display()
                    )),
                ));
            }
        }
    }
    if let Some(key) = &settings.sort_key {
        let mut new_issues = vec![];
        res.sort_by(|(el, _), (er, _)| {
            let (sl, mut il) = el.process(key.to_string());
            let (sr, mut ir) = er.process(key.to_string());
            new_issues.append(&mut il);
            new_issues.append(&mut ir);
            let ord = sl.cmp(&sr);
            if settings.sort_descending {
                ord.reverse()
            } else {
                ord
            }
        });
        for mut issue in new_issues {
            issue.msg += "(occurred while comparing the interpolated sorting-keys)";
            a.push_issue(issue);
        }
    }
    res.into_iter().map(|(_, s)| s).collect::<Vec<_>>().join("")
}
