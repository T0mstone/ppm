use std::convert::identity;
use tlib::iter_tools::{indicator, unescape_all, AutoEscape, Unescape};

fn find_seps(s: &str) -> Vec<usize> {
    let mut lvl = 0usize;
    let mut esc = false;
    let mut res = vec![];
    for i in 0..s.len() {
        if !s.is_char_boundary(i) {
            continue;
        }
        if esc {
            // turn escaping of after one char
            esc = false;
        } else if s[i..].starts_with('\\') {
            esc = true;
        } else if s[i..].starts_with("(%") {
            lvl += 1;
        } else if s[i..].starts_with("%)") {
            lvl = lvl.saturating_sub(1);
        } else if lvl == 0 && s[i..].starts_with(':') {
            res.push(i);
        }
    }
    res
}

fn split_args_inner(mut s: String, mut map: impl FnMut(String) -> String) -> Vec<String> {
    let mut seps = find_seps(&s);
    let mut spl = vec![];
    seps.reverse();
    for i in seps {
        let right = s.split_off(i + 1);
        spl.push(map(right));
        let _sep = s.pop();
    }
    spl.push(s);
    spl.reverse();
    spl
}

fn split_args_with_len_inner(
    mut s: String,
    mut map: impl FnMut(String) -> String,
) -> Vec<(String, usize)> {
    let mut seps = find_seps(&s);
    let mut spl = vec![];
    seps.reverse();
    for i in seps {
        let right = s.split_off(i + 1);
        let len = right.len();
        spl.push((map(right), len));
        let _sep = s.pop();
    }
    let len = s.len();
    spl.push((map(s), len));
    spl.reverse();
    spl
}

/// Also unescapes backslashes
fn unescpae_colons(s: String) -> String {
    s.chars()
        .auto_escape(indicator('\\'))
        .map(|(esc, c)| {
            if c == ':' || c == '\\' {
                (false, c)
            } else {
                (esc, c)
            }
        })
        .unescape(unescape_all('\\'))
        .collect()
}

/// Splits a string according to the separator `:`,respecting any nested command calls
///
/// # Examples
/// - `"a:b:c"` becomes `["a", "b", "c"]`
/// - `"a:(%b c:d%):e"` becomes `["a", "(%b c:d%)", "e"]`
/// - `r"a:\:b:c"` becomes `["a", ":b", "c"]`
/// - `r"a:\(%b:c"` becomes `["a", r"\(%b", "c"]`
/// - `r"a:\\:b:c"` becomes `["a", r"\", "b", "c"]`
#[inline]
pub fn split_args(s: String) -> Vec<String> {
    split_args_inner(s, unescpae_colons)
}

/// Like [`split_args`](function.split_args.html) but splits in such a way
/// that at most `n` parts are created
///
/// # Examples
/// - `"a:b:c:d"` with `n=3` becomes `["a", "b", "c:d"]`
/// - `r"a:b:c:\:d"` with `n=3` becomes `["a", "b", r"c:\:d"]`
pub fn splitn_args(n: usize, s: String) -> Vec<String> {
    if n == 0 {
        return vec![];
    }
    let mut spl = split_args_inner(s, identity);
    if spl.len() > n {
        let tail = spl.split_off(n - 1);
        spl = spl.into_iter().map(unescpae_colons).collect();
        spl.push(tail.join(":"));
    }
    spl
}

/// Like [`split_args`](function.split_args.html) but
/// retains additional information about the original length of each part
#[inline]
pub fn split_args_with_len(s: String) -> Vec<(String, usize)> {
    split_args_with_len_inner(s, unescpae_colons)
}

/// Like [`splitn_args`](function.splitn_args.html) but
/// retains additional information about the original length of each part
pub fn splitn_args_with_len(n: usize, s: String) -> Vec<(String, usize)> {
    if n == 0 {
        return vec![];
    }
    let mut spl = split_args_with_len_inner(s, identity);
    if spl.len() > n {
        let tail = spl.split_off(n - 1);
        spl = spl
            .into_iter()
            .map(|(s, l)| (unescpae_colons(s), l))
            .collect();
        // + 1 because of the separator
        // - 1 is ok since tail can never be empty
        let taillen = tail.iter().map(|(_, l)| l + 1).sum::<usize>() - 1;
        let tail = tail.into_iter().map(|(s, _)| s).collect::<Vec<_>>();
        spl.push((tail.join(":"), taillen));
    }
    spl
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_args() {
        let s = "abc:def(%cmd 1:2:3%):\\(%:\\:heya".to_string();
        let ctrl = vec![
            "abc".to_string(),
            "def(%cmd 1:2:3%)".to_string(),
            "\\(%".to_string(),
            ":heya".to_string(),
        ];
        assert_eq!(split_args(s), ctrl);
    }

    #[test]
    fn test_splitn_args() {
        let s = "abc:def(%cmd 1:2:3%):\\(%:\\:heya".to_string();
        let ctrl = vec![
            "abc".to_string(),
            "def(%cmd 1:2:3%)".to_string(),
            "\\(%:\\:heya".to_string(),
        ];
        assert_eq!(splitn_args(3, s), ctrl);
    }
}
