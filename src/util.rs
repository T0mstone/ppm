use std::collections::VecDeque;
use std::fmt::{Display, Formatter};
use std::iter::Map;
use std::path::{Path, PathBuf};

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct AutoEscaped<I, F> {
    iter: I,
    is_esc: F,
}

impl<I: Iterator, F: FnMut(&I::Item) -> bool> Iterator for AutoEscaped<I, F> {
    type Item = (bool, I::Item);

    fn next(&mut self) -> Option<Self::Item> {
        let nx = self.iter.next()?;
        if (self.is_esc)(&nx) {
            match self.iter.next() {
                Some(t) => Some((true, t)),
                None => Some((false, nx)),
            }
        } else {
            Some((false, nx))
        }
    }
}

pub trait AutoEscape: Sized + IntoIterator {
    fn auto_escape<F: FnMut(&Self::Item) -> bool>(
        self,
        is_esc: F,
    ) -> AutoEscaped<Self::IntoIter, F>;
}

impl<I: IntoIterator> AutoEscape for I {
    #[inline]
    fn auto_escape<F: FnMut(&Self::Item) -> bool>(
        self,
        is_esc: F,
    ) -> AutoEscaped<Self::IntoIter, F> {
        AutoEscaped {
            iter: self.into_iter(),
            is_esc,
        }
    }
}

pub struct Unescaped<T, I, F> {
    iter: I,
    escape_item: F,
    queue: VecDeque<T>,
}

impl<T, I: Iterator<Item = (bool, T)>, J: IntoIterator<Item = T>, F: FnMut(&T) -> J> Iterator
    for Unescaped<T, I, F>
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(x) = self.queue.pop_front() {
            return Some(x);
        }

        let (esc, t) = self.iter.next()?;
        if esc {
            let iter = (self.escape_item)(&t).into_iter().chain(std::iter::once(t));
            self.queue.extend(iter);
            // at least the chained `once` will always produce one value
            Some(self.queue.pop_front().unwrap())
        } else {
            Some(t)
        }
    }
}

pub trait Unescape: Sized + IntoIterator {
    type InnerItem;

    fn unescape_ignore(self)
        -> Map<Self::IntoIter, fn((bool, Self::InnerItem)) -> Self::InnerItem>;

    fn unescape<I: IntoIterator<Item = Self::InnerItem>, F: FnMut(&Self::InnerItem) -> I>(
        self,
        escape_item: F,
    ) -> Unescaped<Self::InnerItem, Self::IntoIter, F>;
}

impl<T, I: IntoIterator<Item = (bool, T)>> Unescape for I {
    type InnerItem = T;

    fn unescape_ignore(self) -> Map<I::IntoIter, fn((bool, Self::InnerItem)) -> Self::InnerItem> {
        self.into_iter().map(|t| t.1)
    }

    fn unescape<J: IntoIterator<Item = Self::InnerItem>, F: FnMut(&Self::InnerItem) -> J>(
        self,
        escape_item: F,
    ) -> Unescaped<Self::InnerItem, Self::IntoIter, F> {
        Unescaped {
            iter: self.into_iter(),
            escape_item,
            queue: VecDeque::new(),
        }
    }
}

#[inline]
pub fn indicator<T: PartialEq>(x: T) -> impl FnMut(&T) -> bool {
    move |t| *t == x
}

#[inline]
pub fn unescaped_indicator<T: PartialEq>(x: T) -> impl FnMut(&(bool, T)) -> bool {
    move |(esc, t)| !*esc && *t == x
}

#[inline]
pub fn unescaped_length<'a, T: 'a>(
    sl: impl IntoIterator<Item = &'a (bool, T)>,
    mut len: impl FnMut(&T) -> usize,
    esc_len: usize,
) -> usize {
    sl.into_iter()
        .map(|tup| len(&tup.1) + if tup.0 { esc_len } else { 0 })
        .sum()
}

#[inline]
pub fn unescape_all_except<T: PartialEq, U: Clone>(t: T, esc: U) -> impl FnMut(&T) -> Vec<U> {
    move |x| if x == &t { vec![] } else { vec![esc.clone()] }
}

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

pub struct SplitIter<I: Iterator, F> {
    // (dyn) the number of split segments already returned
    // starts at 0
    curr_len: usize,
    max_len: Option<usize>,
    iter: I,
    is_sep: F,
    // (setting) whether to emit the separator into the stream
    keep_sep: bool,
    // (dyn) the separator, if it was kept from the last call to `next`
    // starts at None
    last_sep: Option<I::Item>,
}

impl<I: Iterator, F: FnMut(&I::Item) -> bool> Iterator for SplitIter<I, F> {
    type Item = Vec<I::Item>;

    fn next(&mut self) -> Option<Vec<I::Item>> {
        if let Some(sep) = self.last_sep.take() {
            return Some(vec![sep]);
        }
        self.curr_len += 1;
        if self.max_len.map_or(false, |len| self.curr_len == len) {
            // the length limit is reached: return the whole rest
            return Some(self.iter.by_ref().collect());
        }
        let mut res: Option<Vec<I::Item>> = None;
        while let Some(x) = self.iter.next() {
            if res.is_none() {
                res = Some(Vec::new());
            }
            if (self.is_sep)(&x) {
                if self.keep_sep {
                    self.last_sep = Some(x);
                }
                break;
            } else {
                match res.as_mut() {
                    Some(v) => v.push(x),
                    None => res = Some(vec![x]),
                }
            }
        }
        res
    }
}

pub trait IterSplit: Sized + IntoIterator {
    fn split_impl<F: FnMut(&Self::Item) -> bool>(
        self,
        max_len: Option<usize>,
        is_sep: F,
        keep_sep: bool,
    ) -> SplitIter<Self::IntoIter, F>;

    fn split<F: FnMut(&Self::Item) -> bool>(
        self,
        is_sep: F,
        keep_sep: bool,
    ) -> SplitIter<Self::IntoIter, F> {
        self.split_impl(None, is_sep, keep_sep)
    }

    fn splitn<F: FnMut(&Self::Item) -> bool>(
        self,
        n: usize,
        is_sep: F,
        keep_sep: bool,
    ) -> SplitIter<Self::IntoIter, F> {
        self.split_impl(Some(n), is_sep, keep_sep)
    }
}

impl<I: IntoIterator> IterSplit for I {
    fn split_impl<F: FnMut(&Self::Item) -> bool>(
        self,
        max_len: Option<usize>,
        is_sep: F,
        keep_sep: bool,
    ) -> SplitIter<Self::IntoIter, F> {
        SplitIter {
            curr_len: 0,
            max_len,
            iter: self.into_iter(),
            is_sep,
            keep_sep,
            last_sep: None,
        }
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
            .split_impl(max_len, unescaped_indicator(sep), keep_sep)
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
