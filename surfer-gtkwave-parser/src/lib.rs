//! # surfer-gtkwave-parser
//!
//! Try to parse a GTKWave dump file into Surfer messages, on a best-effort basis.
//!
//! This is not a general GTKWave dump file parser. We only care about the directives that make
//! sense for surfer.

use aho_corasick::AhoCorasick;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Color {
    Cycle,
    Normal,
    Red,
    Orange,
    Yellow,
    Green,
    Blue,
    Indigo,
    Violet,
}

impl TryFrom<i32> for Color {
    type Error = ();

    fn try_from(value: i32) -> std::result::Result<Self, Self::Error> {
        use Color::*;
        match value {
            -1 => Ok(Cycle),
            0 => Ok(Normal),
            1 => Ok(Red),
            2 => Ok(Orange),
            3 => Ok(Yellow),
            4 => Ok(Green),
            5 => Ok(Blue),
            6 => Ok(Indigo),
            7 => Ok(Violet),
            _ => Err(()),
        }
    }
}

#[derive(Debug)]
pub enum Directive {
    /// Path to wave file to open
    Dumpfile(String),
    Trace {
        path: String,
        bit_range: Option<(u32, u32)>,
        color: Option<Color>,
        flags: Option<Flags>,
    },
}

#[derive(Debug)]
pub enum Error {
    Eof,
    UnknownLine(String),
    UnknownDirective(String),
    Other(String),
}

impl From<String> for Error {
    fn from(value: String) -> Error {
        Error::Other(value)
    }
}

bitflags::bitflags! {
    #[derive(Debug)]
    pub struct Flags: u32 {
        /// Highlight the trace item
        const highlight = 1;
        /// Hexadecimal data value representation
        const hex = 1 << 1;
        /// Decimal data value representation
        const dec = 1 << 2;
        /// Binary data value representation
        const bin = 1 << 3;
        /// Octal data value representation
        const oct = 1 << 4;

        /// Right-justify signal name/alias
        const rjustify = 1 << 5;
        const invert = 1 << 6;
        const reverse = 1 << 7;
        const exclude = 1 << 8;
        const blank = 1 << 9;

        /// Signed (2's complement) data representation
        const signed = 1 << 10;
        /// ASCII character representation
        const ascii = 1 << 11;

        const collapsed = 1 << 12;
        /// Trace translated with filter file
        const ftranslated = 1 << 13;
        /// Trace translated with filter process
        const ptranslated = 1 << 14;

        const analog_step = 1 << 15;
        const analog_interpolated = 1 << 16;
        const analog_blank_stretch = 1 << 17;
        const real = 1 << 18;
        const analog_fullscale = 1 << 19;
        const zerofill = 1 << 20;
        const onefill = 1 << 21;
        const closed = 1 << 22;

        /// Begin a group of signals
        const grp_begin = 1 << 23;
        /// End a group of signals
        const grp_end = 1 << 24;

        const bingray = 1 << 25;
        const graybin = 1 << 26;
        const real2bits = 1 << 27;
        const ttranslated = 1 << 28;

        /// Show the population count, i.e. the number of set bits
        const popcnt = 1 << 29;

        const fpdecshift = 1 << 30;
    }
}

pub struct Parser<'s> {
    lines: Vec<&'s str>,
    current: usize,
}

impl<'s> Parser<'s> {
    pub fn new(content: &'s str) -> Self {
        Self {
            lines: content.lines().collect(),
            current: 0,
        }
    }

    pub fn parse(mut self) -> (Vec<Directive>, Vec<Error>) {
        let mut directives = vec![];
        let mut errors = vec![];

        while self.peek_line().is_ok() {
            match self.next() {
                Ok(dir) => directives.push(dir),
                Err(e) => errors.push(e),
            }
            self.next_line().ok();
        }

        (directives, errors)
    }

    fn peek_line(&self) -> Result<&str> {
        self.lines.get(self.current).copied().ok_or(Error::Eof)
    }

    fn next_line(&mut self) -> Result<&str> {
        let line = self.lines.get(self.current).copied().ok_or(Error::Eof)?;
        self.current += 1;
        Ok(line)
    }

    fn next(&mut self) -> Result<Directive> {
        let line = self.peek_line()?;
        let dir = if line.starts_with('[') {
            self.bracketed()?
        } else if let Some(trace) = self.try_trace()? {
            trace
        } else {
            Err(Error::UnknownLine(self.peek_line()?.to_string()))?
        };
        Ok(dir)
    }

    fn bracketed(&mut self) -> Result<Directive> {
        let patterns = &[("[dumpfile]", Self::dumpfile)];
        let ac = AhoCorasick::new(patterns.iter().map(|(s, _)| s)).unwrap();
        let line = self.peek_line()?;
        let m = ac
            .find(line)
            .ok_or_else(|| Error::UnknownDirective(line.to_string()))?;
        let id = m.pattern().as_usize();
        let f = patterns[id].1;
        f(self)
    }

    fn dumpfile(&mut self) -> Result<Directive> {
        let line = self.peek_line()?;
        assert!(line.starts_with("[dumpfile]"));
        let (_, rest) = line.split_at("[dumpfile]".len());
        // trim ` "` and `"`
        let path = &rest[2..rest.len() - 1];
        Ok(Directive::Dumpfile(path.to_string()))
    }

    fn try_trace(&mut self) -> Result<Option<Directive>> {
        // TODO: loop try_[flag,color,...] until a trace shows up (using a combinator)
        let flags = self.try_flags()?;
        if flags.is_some() {
            self.next_line()?;
        }
        let color = self.try_color()?;
        if color.is_some() {
            self.next_line()?;
        }

        let line = self.peek_line()?;
        // heurestic: lines containing spaces are probably not traces
        // FIXME: this could be better
        if flags.is_none() && color.is_none() && line.contains(' ') {
            return Ok(None);
        }

        let (path, bit_range) = if line.contains('[') {
            let (path, bit_range) = line.split_once('[').unwrap();
            // trim `]`
            let bit_range = &bit_range[0..bit_range.len() - 1];
            let (start, end) = bit_range
                .split_once(':')
                .ok_or_else(|| format!("Didn't understand bit range in trace: '{bit_range}'"))?;
            dbg!(bit_range, start, end);
            let start = start
                .parse()
                .map_err(|e| format!("Couldn't parse start of bit range '{bit_range}': {e}"))?;
            let end = end
                .parse()
                .map_err(|e| format!("Couldn't parse end of bit range '{bit_range}': {e}"))?;
            (path.to_string(), Some((start, end)))
        } else {
            (line.to_string(), None)
        };

        Ok(Some(Directive::Trace {
            path,
            bit_range,
            color,
            flags,
        }))
    }

    fn try_color(&self) -> Result<Option<Color>> {
        let line = self.peek_line()?;
        if line.starts_with("[color] ") {
            let (_, rest) = line.split_once(' ').unwrap();
            let color_int: i32 = rest
                .parse()
                .map_err(|e| format!("Couldn't parse color '{rest}' as int: {e}"))?;
            let color = Color::try_from(color_int)
                .map_err(|()| format!("Invalid color integer '{color_int}'"))?;
            Ok(Some(color))
        } else {
            Ok(None)
        }
    }

    fn try_flags(&self) -> Result<Option<Flags>> {
        let line = self.peek_line()?;
        if line.starts_with('@') {
            let value = line.strip_prefix('@').unwrap();
            let bits = u32::from_str_radix(value, 16)
                .map_err(|e| format!("Couldn't parse flags '{value}' as u32: {e}"))?;
            let flags = Flags::from_bits(bits)
                .ok_or_else(|| format!("Invalid value for flags: '{bits}'"))?;
            Ok(Some(flags))
        } else {
            Ok(None)
        }
    }
}
