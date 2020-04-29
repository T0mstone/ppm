// use std::collections::VecDeque;
use std::fmt::{Display, Formatter};
// use std::iter::Map;
use std::path::{Path, PathBuf};

pub use tlib::iter_tools::{
    indicator, indicator_not_escaped, unescape_all_except, AutoEscape, AutoEscapeIter, IterSplit,
    SplitIter, Unescape, UnescapeIter,
};

// counts left and right delimeters, stopping once the counter goes below zero
// (returns the final right delimeter depending on `emit_final`)
pub struct TakeWhileLevelGe0<'a, I: ?Sized, F, G> {
    emit_final: bool,
    iter: &'a mut I,
    lvl: Option<usize>,
    is_inc: F,
    is_dec: G,
}

impl<'a, I: Iterator, P: FnMut(&I::Item) -> bool, Q: FnMut(&I::Item) -> bool> Iterator
    for TakeWhileLevelGe0<'a, I, P, Q>
{
    type Item = I::Item;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let lvl = self.lvl.as_mut()?;
        let nx = self.iter.next()?;
        if (self.is_inc)(&nx) {
            *lvl += 1;
        } else if (self.is_dec)(&nx) {
            if *lvl == 0 {
                self.lvl = None;
                if !self.emit_final {
                    return None;
                }
            } else {
                *lvl -= 1;
            }
        }
        Some(nx)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        if self.lvl.is_none() {
            (0, Some(0))
        } else {
            let (_, upper) = self.iter.size_hint();
            (0, upper)
        }
    }
}

pub trait CreateTakeWhileLevelGe0<P, Q> {
    fn take_while_lvl_ge0(
        &mut self,
        is_inc: P,
        is_dec: Q,
        emit_final: bool,
    ) -> TakeWhileLevelGe0<Self, P, Q>;
}

impl<I: Iterator, P: FnMut(&I::Item) -> bool, Q: FnMut(&I::Item) -> bool>
    CreateTakeWhileLevelGe0<P, Q> for I
{
    fn take_while_lvl_ge0(
        &mut self,
        is_inc: P,
        is_dec: Q,
        emit_final: bool,
    ) -> TakeWhileLevelGe0<Self, P, Q> {
        TakeWhileLevelGe0 {
            iter: self,
            lvl: Some(0),
            is_inc,
            is_dec,
            emit_final,
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct FileLoc {
    pub row: usize,
    pub col: usize,
}

impl Display for FileLoc {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        (self.row + 1).fmt(f)?;
        f.pad(":")?;
        (self.col + 1).fmt(f)
    }
}

impl FileLoc {
    pub fn from_index(i: usize, s: &str) -> Self {
        // the 'cursor' sits right before the char at `pos`, thus the range needs to be exclusive
        let before = &s[..i];

        let row = before.chars().filter(indicator('\n')).count();
        let col = before.chars().rev().take_while(|&c| c != '\n').count();
        Self { row, col }
    }

    pub fn to_index(&self, s: &str) -> usize {
        let i: usize = s.split('\n').take(self.row).map(|s| s.len()).sum();
        // no rows: i == 0
        // one row: i == (row 1).len()
        //      -> "abcd\n".len() == 5 -> i == 5
        //      -> i points after the newline, just as i want
        i + self.col
    }
}

#[derive(Default, Debug, Copy, Clone, Eq, PartialEq)]
pub struct Span {
    pub start: usize,
    pub len: usize,
}

impl Span {
    #[inline]
    pub fn new(start: usize, len: usize) -> Self {
        Self { start, len }
    }

    #[inline]
    pub fn empty() -> Self {
        Self::default()
    }

    #[inline]
    /// Returns `idx + len`, so the position *right after* the end of the span
    pub fn end(&self) -> usize {
        self.start + self.len
    }

    #[inline]
    pub fn lengthen(&mut self, d: usize) {
        self.len += d;
    }

    #[inline]
    pub fn index_str<'a>(&self, s: &'a str) -> &'a str {
        &s[self.start..self.end()]
    }

    #[inline]
    pub fn start_end_loc(&self, s: &str) -> (FileLoc, FileLoc) {
        (
            FileLoc::from_index(self.start, s),
            FileLoc::from_index(self.end(), s),
        )
    }

    #[inline]
    pub fn shift_start_forward(&mut self, shift: usize) {
        self.start += shift;
        self.len -= shift;
    }
}

#[inline]
pub fn head_tail<T>(mut v: Vec<T>) -> Option<(T, Vec<T>)> {
    if v.is_empty() {
        None
    } else {
        Some((v.remove(0), v))
    }
}

pub trait SplitUnescString {
    fn split_unescaped_impl(
        &self,
        max_len: Option<usize>,
        sep: char,
        esc: char,
        keep_sep: bool,
    ) -> Vec<String>;

    #[inline]
    fn split_unescaped(&self, sep: char, esc: char, keep_sep: bool) -> Vec<String> {
        self.split_unescaped_impl(None, sep, esc, keep_sep)
    }

    #[inline]
    fn splitn_unescaped(&self, n: usize, sep: char, esc: char, keep_sep: bool) -> Vec<String> {
        self.split_unescaped_impl(Some(n), sep, esc, keep_sep)
    }
}

impl<S: AsRef<str>> SplitUnescString for S {
    fn split_unescaped_impl(
        &self,
        max_len: Option<usize>,
        sep: char,
        esc: char,
        keep_sep: bool,
    ) -> Vec<String> {
        self.as_ref()
            .chars()
            .auto_escape(indicator(esc))
            .split_impl(max_len, indicator_not_escaped(sep), keep_sep)
            .map(|v| {
                v.into_iter()
                    .unescape(unescape_all_except(sep, esc))
                    .collect::<String>()
            })
            .collect()
    }
}

#[inline]
pub fn make_absolute<P: AsRef<Path>>(p: P, dir: Option<PathBuf>) -> std::io::Result<PathBuf> {
    let p = p.as_ref();
    if p.is_absolute() {
        return Ok(p.to_path_buf());
    }
    match dir {
        Some(mut d) => {
            d.push(p);
            Ok(d)
        }
        None => {
            let mut d = std::env::current_dir()?;
            d.push(p);
            Ok(d)
        }
    }
}
