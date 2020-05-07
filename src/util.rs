use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};

pub use tlib::iter_tools::{
    indicator, indicator_not_escaped, unescape_all_except, AutoEscape, AutoEscapeIter, IterSplit,
    SplitIter, SplitNotEscapedString, Unescape, UnescapeIter,
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

/// A position in a string, split into row and column
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct RowCol {
    /// The row (a.k.a. line) number
    pub row: usize,
    /// The column number
    pub col: usize,
}

impl Display for RowCol {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        (self.row + 1).fmt(f)?;
        f.pad(":")?;
        (self.col + 1).fmt(f)
    }
}

impl RowCol {
    /// Calculates a `RowCol` from an index and a corresponding `str`
    pub fn from_index(i: usize, s: &str) -> Self {
        // the 'cursor' sits right before the char at `pos`, thus the range needs to be exclusive
        let before = &s[..i];

        let row = before.chars().filter(indicator('\n')).count();
        let col = before.chars().rev().take_while(|&c| c != '\n').count();
        Self { row, col }
    }

    /// Calculates an index from a `RowCol` and a corresponding `str`
    pub fn to_index(&self, s: &str) -> usize {
        let i: usize = s.split('\n').take(self.row).map(|s| s.len()).sum();
        // no rows: i == 0
        // one row: i == (row 1).len()
        //      -> "abcd\n".len() == 5 -> i == 5
        //      -> i points after the newline, just as i want
        i + self.col
    }
}

/// A region in a string
#[derive(Default, Debug, Copy, Clone, Eq, PartialEq)]
pub struct Span {
    /// the starting index
    pub start: usize,
    /// the length
    pub len: usize,
}

impl Span {
    /// Creates a new `Span`
    #[inline]
    pub fn new(start: usize, len: usize) -> Self {
        Self { start, len }
    }

    /// Creates a new `Span` at position `0` with length `0`
    #[inline]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Returns `idx + len`, so the position *right after* the end of the span
    #[inline]
    pub fn end(&self) -> usize {
        self.start + self.len
    }

    /// Indexes into a `str`
    #[inline]
    pub fn index_str<'a>(&self, s: &'a str) -> &'a str {
        &s[self.start..self.end()]
    }

    /// Splits the span into a pair of [`RowCol`](struct.RowCol)s,
    /// the first one at the start and the second on at the end of the Span
    #[inline]
    pub fn start_end_loc(&self, s: &str) -> (RowCol, RowCol) {
        (
            RowCol::from_index(self.start, s),
            RowCol::from_index(self.end(), s),
        )
    }

    /// Shifts the starting point forward without affecting the end
    #[inline]
    pub fn shift_start_forward(&mut self, shift: usize) {
        self.start += shift;
        self.len -= shift;
    }

    /// Treats `self` as starting from `other.start` and returns the absolute span of `self`
    ///
    /// # Fails
    /// Returns `None` when `self` is too long to fit into `other`
    #[inline]
    pub fn relative_to(mut self, other: &Self) -> Option<Self> {
        self.start += other.start;
        if self.end() <= other.end() {
            Some(self)
        } else {
            None
        }
    }

    /// Creates a span with `self.start` and `len`
    #[inline]
    pub fn with_length(self, len: usize) -> Self {
        Self { len, ..self }
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
