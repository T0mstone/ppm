use crate::util::{
    indicator, indicator_not_escaped, unescape_all_except, AutoEscape, CreateTakeWhileLevelGe0,
    Unescape,
};

pub fn split_args(s: &str) -> Vec<String> {
    let mut iter = s.chars().auto_escape(indicator('\\'));
    let mut res = vec![String::new()];
    while let Some((esc, c)) = iter.next() {
        let last = res.last_mut().unwrap();
        match c {
            '"' if !esc => {
                last.extend(
                    iter.take_while_lvl_ge0(|_| false, indicator_not_escaped('"'), false)
                        .unescape(unescape_all_except('"', '\\')),
                );
            }
            ' ' => {
                if !last.is_empty() {
                    res.push(String::new());
                }
            }
            c => last.push(c),
        }
    }
    res
}

pub fn matches_pattern(mut s: &str, pat: &str) -> bool {
    let pat = pat.chars().enumerate().map(|(i, c)| (i == 0, c));
    let mut last_star = false;
    for (first, c) in pat {
        if c == '*' {
            last_star = true;
        } else {
            last_star = false;
            match s.find(c) {
                Some(i) => {
                    if first && i != 0 {
                        return false;
                    }
                    s = &s[i + 1..];
                }
                None => {
                    return false;
                }
            }
        }
    }
    last_star || s.is_empty()
}
