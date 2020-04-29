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

// pub fn split_args_ranges(s: &str) -> Vec<Span> {
//     let mut iter = s.chars().auto_escape(indicator('\\'));
//     let mut res = vec![Span::empty()];
//     while let Some((esc, c)) = iter.next() {
//         let last = res.last_mut().unwrap();
//         match c {
//             '"' if !esc => {
//                 let len: usize = iter
//                     .take_while_lvl_ge0(|_| false, |&(esc, c)| !esc && c == '"', true)
//                     .map(|(esc, c)| c.len_utf8() + if esc { 1 } else { 0 })
//                     .sum();
//                 // the + 1 because `len` doesn't count the '"' we already used
//                 last.lengthen(len + 1);
//             }
//             ' ' => {
//                 if last.len == 0 {
//                     // two spaces back-to-back
//                     // the first of them is ignored
//                     // thus we transform its entry into the new entry
//                     last.start += 1;
//                 } else {
//                     let span = Span::new(last.start + 1, 0);
//                     res.push(span);
//                 }
//             }
//             c => last.lengthen(c.len_utf8()),
//         }
//     }
//     res
// }

pub fn matches_pattern(mut s: &str, pat: &str) -> bool {
    let mut pat = pat.chars().enumerate().map(|(i, c)| (i == 0, c));
    let mut last_star = false;
    while let Some((first, c)) = pat.next() {
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
