use crate::util::{indicator, AutoEscape, CreateTakeWhileLevelGe0, Unescape};
use crate::{Issue, State};
use std::collections::HashMap;
use std::iter::once;

pub fn replace_all(s: &mut State, vars: &HashMap<String, String>) {
    while let Some((_, (i, _))) = s
        .char_indices()
        .auto_escape(|t| t.1 == '\\')
        .find(|&(esc, (i, _))| !esc && s[i..].starts_with("%{"))
    {
        if i + 2 == s.len() {
            s.issues.push(Issue {
                id: "strvar:incomplete",
                msg: "incomplete variable insertion ignored".to_string(),
                loc: s.calc_position(i),
            });
            continue;
        }
        let mut iter = s[i + 2..].chars().auto_escape(indicator('\\')).peekable();

        let arg = iter
            .take_while_lvl_ge0(
                |&(esc, c)| !esc && c == '{',
                |&(esc, c)| !esc && c == '}',
                false,
            )
            .unescape(|_| once('\\'))
            .collect::<String>();
        //   %{abc}
        // i-^ [-] ^- end
        //      ^- arg.len()
        let end = i + arg.len() + 3;
        let res = match vars.get(&arg) {
            Some(s) => s,
            None => {
                s.issues.push(Issue {
                    id: "strvar:unknown_var",
                    msg: format!("unknown variable: {}", arg),
                    loc: s.calc_position(i),
                });
                ""
            }
        };
        s.replace_range(i..end, res)
    }
}
