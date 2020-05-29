use std::borrow::Cow;
use std::cell::RefCell;
use std::cmp::max;
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::fmt;
use std::hash::Hash;

use thiserror::Error;

pub struct TableFormat<T> {
    sizes: RefCell<HashMap<T, usize>>,
    format: Vec<FormatPart<T>>,
}

impl<T, D> TableFormat<T>
    where T: Column<Data=D> + Copy {
    fn new(format: Vec<FormatPart<T>>) -> Self {
        TableFormat { sizes: Default::default(), format }
    }

    pub fn parse_spec(s: &str) -> Result<Self, ParseError> {
        let mut parts = Vec::new();
        let mut partial = String::new();
        let mut rem = s;
        while let Some(next_bound) = rem.find('%') {
            partial.push_str(&rem[..next_bound]);
            rem = &rem[next_bound + 1..];
            let ch = rem.chars().next().ok_or(ParseError::Eof)?;
            if ch == '%' {
                partial.push(ch);
            } else {
                parts.push(FormatPart::Literal(partial.to_string()));
                partial = String::new();
                parts.push(FormatPart::Column(T::by_char(ch).ok_or(ParseError::InvalidPart(ch))?));
            }
            rem = &rem[ch.len_utf8()..];
        }
        partial.push_str(rem);
        parts.push(FormatPart::Literal(partial.to_string()));
        Ok(TableFormat::new(parts))
    }

    pub fn bind<'a>(&'a self, row: &'a D) -> BoundTable<'a, T> {
        BoundTable { table: self, data: row }
    }

    pub fn format_row(&self, row: &D) -> String {
        format!("{}", self.bind(row))
    }
}

pub struct BoundTable<'a, T>
    where T: Column {
    table: &'a TableFormat<T>,
    data: &'a T::Data,
}

impl<T> Display for BoundTable<'_, T>
    where T: Column + Copy {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        for part in self.table.format.iter() {
            match part {
                FormatPart::Column(c) if !c.constant_size() => {
                    let col = part.to_string(self.data);
                    let len = col.chars().count();
                    let col_width = *self.table.sizes.borrow_mut().entry(*c)
                        .and_modify(|v| *v = max(*v, len))
                        .or_insert(len);

                    match c.alignment() {
                        Alignment::Left => {
                            write!(f, "{}{}", col, " ".repeat(col_width - len))?;
                        }
                        Alignment::Right => {
                            write!(f, "{}{}", " ".repeat(col_width - len), col)?;
                        }
                    }
                }
                _ => {
                    write!(f, "{}", part.to_string(self.data))?;
                }
            }
        }
        Ok(())
    }
}

#[derive(Error, Debug, Copy, Clone)]
pub enum ParseError {
    #[error("invalid format spec %{0}")]
    InvalidPart(char),
    #[error("unexpected end of format string")]
    Eof,
}

enum FormatPart<T> {
    Literal(String),
    Column(T),
}

impl<T, D> FormatPart<T>
    where T: Column<Data=D> {
    fn to_string<'a>(&'a self, row: &'a D) -> Cow<'a, str> {
        match self {
            FormatPart::Literal(l) => l.into(),
            FormatPart::Column(c) => c.to_string(row),
        }
    }
}

pub trait Column: Eq + Hash + Sized {
    type Data;

    fn by_char(ch: char) -> Option<Self>;

    fn to_string<'a>(&'a self, data: &'a Self::Data) -> Cow<'a, str>;

    fn constant_size(&self) -> bool { false }

    fn alignment(&self) -> Alignment { Alignment::Left }
}

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub enum Alignment {
    Left,
    Right,
}