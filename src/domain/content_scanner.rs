use std::collections::BTreeMap;

use lopdf::Encoding;

/// Scans a page content stream and produces its extracted text in a single
/// pass, mirroring lopdf's operator handling (same semantics as
/// `extract_text_with_limit`) without materializing the full `Vec<Operation>`
/// tree that `Content::decode` builds.
///
/// The tokenizer replicates loss-for-loss the `content` grammar of
/// `lopdf::parser` (see `parser/mod.rs`): operands are parsed identically
/// (strings, hex strings, names, numbers, arrays, dictionaries, booleans,
/// null, references, inline images) and any parse failure stops the stream at
/// the same point lopdf does, dropping the remainder exactly like
/// `strip_nom(Content::decode)`.
pub(crate) fn extract_page_text(content: &[u8], encodings: &BTreeMap<Vec<u8>, Encoding>) -> String {
    let mut parser = Parser::new(content);
    let mut text = String::new();
    let mut chunks = String::new();
    let mut current_encoding: Option<&Encoding> = None;
    let mut page_error = false;
    let mut operands: Vec<Operand> = Vec::new();

    'ops: loop {
        parser.skip_ws();
        while parser.peek() == Some(b'%') {
            parser.skip_comment();
            parser.skip_ws();
        }
        if parser.peek().is_none() {
            break;
        }
        operands.clear();

        // An inline image is only attempted at the start of an operation
        // (before any operand), exactly like lopdf's `alt((inline_image, ...))`.
        if content.get(parser.pos..parser.pos + 2) == Some(b"BI") {
            match parser.inline_image() {
                Ok(()) => continue,
                Err(Flow::Fail) => return String::new(),
                Err(Flow::Stop) => break,
            }
        }

        loop {
            match parser.step(&mut operands) {
                Ok(Step::Operand) => parser.skip_ws(),
                Ok(Step::Operator(start, end)) => {
                    match &content[start..end] {
                        b"Tf" => {
                            current_encoding = match operands.first() {
                                Some(Operand::Name(name)) => encodings.get(name),
                                _ => {
                                    page_error = true;
                                    None
                                }
                            };
                            if !chunks.is_empty() {
                                text.push_str(&chunks);
                                chunks.clear();
                            }
                        }
                        b"Tj" | b"TJ" => {
                            if let Some(encoding) = current_encoding
                                && collect_text(&mut chunks, encoding, &operands).is_err()
                            {
                                page_error = true;
                            }
                        }
                        b"'" => {
                            if let Some(encoding) = current_encoding {
                                if !chunks.ends_with('\n') {
                                    chunks.push('\n');
                                }
                                if collect_text(&mut chunks, encoding, &operands).is_err() {
                                    page_error = true;
                                }
                            }
                        }
                        b"\"" => {
                            if let Some(encoding) = current_encoding {
                                if !chunks.ends_with('\n') {
                                    chunks.push('\n');
                                }
                                if let Some(string_operand) = operands.get(2)
                                    && collect_text(
                                        &mut chunks,
                                        encoding,
                                        std::slice::from_ref(string_operand),
                                    )
                                    .is_err()
                                {
                                    page_error = true;
                                }
                            }
                        }
                        b"T*" if !chunks.ends_with('\n') => chunks.push('\n'),
                        b"T*" => {}
                        b"ET" if !chunks.ends_with('\n') => chunks.push('\n'),
                        b"ET" => {}
                        _ => {}
                    }
                    continue 'ops;
                }
                Err(Flow::Stop) => break 'ops,
                Err(Flow::Fail) => return String::new(),
            }
        }
    }

    if !chunks.is_empty() {
        text.push_str(&chunks);
    }
    if page_error { String::new() } else { text }
}

/// Decodes text operands with the active encoding, matching lopdf's
/// `collect_text`: array operands recurse and add a space; a negative advance
/// of more than 100 adds a space.
fn collect_text(text: &mut String, encoding: &Encoding, operands: &[Operand]) -> Result<(), ()> {
    for operand in operands {
        match operand {
            Operand::Str(bytes) => encoding.write_to_string(bytes, text).map_err(|_| ())?,
            Operand::Array(items) => {
                collect_text(text, encoding, items)?;
                text.push(' ');
            }
            Operand::Int(value) if *value < -100 => text.push(' '),
            _ => {}
        }
    }
    Ok(())
}

/// Matches lopdf's nested-value budgets (`MAX_NESTING_DEPTH` / `MAX_BRACKET`).
const MAX_DEPTH: usize = 100;

const CONTENT_SPACE: &[u8] = b" \t\r\n";
const WHITESPACE: &[u8] = b" \t\r\n\0\x0c";
const DELIMITERS: &[u8] = b"()<>[]{}/%";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Flow {
    /// Hard stop: lopdf's `many0(operation)` stops on `Err::Error`, keeping
    /// the operations parsed so far and dropping the remaining input.
    Stop,
    /// `Err::Failure` (via `cut`): the whole content stream fails.
    Fail,
}

enum Step {
    Operand,
    Operator(usize, usize),
}

enum Operand {
    Str(Vec<u8>),
    Name(Vec<u8>),
    Array(Vec<Operand>),
    Int(i64),
    /// Parsed and discarded: real operands never affect extracted text.
    Real,
    Bool(bool),
    Null,
    Ref,
    Dict,
}

impl Operand {
    fn as_int(&self) -> Option<i64> {
        match self {
            Operand::Int(v) => Some(*v),
            _ => None,
        }
    }

    fn as_bool(&self) -> Option<bool> {
        match self {
            Operand::Bool(v) => Some(*v),
            _ => None,
        }
    }

    fn as_name(&self) -> Option<&[u8]> {
        match self {
            Operand::Name(v) => Some(v),
            _ => None,
        }
    }
}

struct Parser<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    #[inline]
    fn peek(&self) -> Option<u8> {
        self.data.get(self.pos).copied()
    }

    #[inline]
    fn peek_at(&self, offset: usize) -> Option<u8> {
        self.data.get(self.pos + offset).copied()
    }

    #[inline]
    fn starts_with(&self, bytes: &[u8]) -> bool {
        self.data.get(self.pos..self.pos + bytes.len()) == Some(bytes)
    }

    /// ` `, `\t`, `\r`, `\n` only (lopdf `content_space`).
    #[inline]
    fn skip_ws(&mut self) {
        while self.peek().is_some_and(|c| CONTENT_SPACE.contains(&c)) {
            self.pos += 1;
        }
    }

    /// ` `, `\t`, `\r`, `\n`, `\0`, `\x0c` (lopdf `white_space`).
    #[inline]
    fn skip_all_ws(&mut self) {
        while self.peek().is_some_and(|c| WHITESPACE.contains(&c)) {
            self.pos += 1;
        }
    }

    /// Whitespace plus comments, consuming a comment with its EOL (lopdf
    /// `space`, used inside arrays/dictionaries and around image dicts).
    fn skip_space(&mut self) {
        loop {
            self.skip_all_ws();
            if self.peek() == Some(b'%') {
                self.skip_comment();
            } else {
                break;
            }
        }
    }

    /// Consumes from `%` to EOL (inclusive). `\r\n` is a single EOL.
    fn skip_comment(&mut self) {
        self.pos += 1;
        while let Some(b) = self.peek() {
            match b {
                b'\n' => {
                    self.pos += 1;
                    return;
                }
                b'\r' => {
                    self.pos += 1;
                    if self.peek() == Some(b'\n') {
                        self.pos += 1;
                    }
                    return;
                }
                _ => self.pos += 1,
            }
        }
    }

    /// One content-stream operand, or once the operand run ends, the operator
    /// keyword (rendered as a byte range into the stream).
    fn step(&mut self, operands: &mut Vec<Operand>) -> Result<Step, Flow> {
        match self.peek() {
            Some(b'(') => operands.push(Operand::Str(self.literal_string(MAX_DEPTH)?)),
            Some(b'<') => {
                if self.peek_at(1) == Some(b'<') {
                    self.dictionary(MAX_DEPTH)?;
                    operands.push(Operand::Dict);
                } else {
                    operands.push(Operand::Str(self.hex_string()?));
                }
            }
            Some(b'[') => operands.push(Operand::Array(self.array(MAX_DEPTH)?)),
            Some(b'/') => operands.push(Operand::Name(self.name()?)),
            Some(c) if c.is_ascii_digit() || c == b'+' || c == b'-' || c == b'.' => {
                operands.push(self.number()?);
            }
            Some(c) if c.is_ascii_alphabetic() || c == b'*' || c == b'\'' || c == b'"' => {
                let start = self.pos;
                while self.peek().is_some_and(|d| {
                    d.is_ascii_alphabetic() || d == b'*' || d == b'\'' || d == b'"'
                }) {
                    self.pos += 1;
                }
                match &self.data[start..self.pos] {
                    b"true" => operands.push(Operand::Bool(true)),
                    b"false" => operands.push(Operand::Bool(false)),
                    b"null" => operands.push(Operand::Null),
                    _ => return Ok(Step::Operator(start, self.pos)),
                }
            }
            _ => return Err(Flow::Stop),
        }
        Ok(Step::Operand)
    }

    /// `/Name#20abc` — lopdf `name`: regular chars plus `#xx` escapes.
    fn name(&mut self) -> Result<Vec<u8>, Flow> {
        self.pos += 1;
        let mut out = Vec::new();
        loop {
            match self.peek() {
                Some(b'#') => {
                    let (Some(h1), Some(h2)) = (self.peek_at(1), self.peek_at(2)) else {
                        return Err(Flow::Stop);
                    };
                    let (Some(a), Some(b)) = (hex_value(h1), hex_value(h2)) else {
                        return Err(Flow::Stop);
                    };
                    out.push(a << 4 | b);
                    self.pos += 3;
                }
                Some(c) if is_regular(c) => {
                    out.push(c);
                    self.pos += 1;
                }
                _ => return Ok(out),
            }
        }
    }

    /// `( ... )` literal string with nested parens, escapes and raw EOLs.
    fn literal_string(&mut self, depth: usize) -> Result<Vec<u8>, Flow> {
        if depth == 0 {
            return Err(Flow::Stop);
        }
        self.pos += 1;
        let mut out = Vec::new();
        loop {
            match self.peek() {
                None => return Err(Flow::Stop),
                Some(b')') => {
                    self.pos += 1;
                    return Ok(out);
                }
                Some(b'(') => {
                    let nested = self.literal_string(depth - 1)?;
                    out.push(b'(');
                    out.extend_from_slice(&nested);
                    out.push(b')');
                }
                Some(b'\\') => {
                    self.pos += 1;
                    match self.peek() {
                        None => return Err(Flow::Stop),
                        Some(b) if (b'0'..=b'7').contains(&b) => {
                            let mut value: u16 = 0;
                            for _ in 0..3 {
                                match self.peek() {
                                    Some(o) if (b'0'..=b'7').contains(&o) => {
                                        value = value * 8 + (o - b'0') as u16;
                                        self.pos += 1;
                                    }
                                    _ => break,
                                }
                            }
                            out.push(value as u8);
                        }
                        Some(b'\r') => {
                            self.pos += 1;
                            if self.peek() == Some(b'\n') {
                                self.pos += 1;
                            }
                        }
                        Some(b'\n') => {
                            self.pos += 1;
                        }
                        Some(b'n') => {
                            out.push(b'\n');
                            self.pos += 1;
                        }
                        Some(b'r') => {
                            out.push(b'\r');
                            self.pos += 1;
                        }
                        Some(b't') => {
                            out.push(b'\t');
                            self.pos += 1;
                        }
                        Some(b'b') => {
                            out.push(0x08);
                            self.pos += 1;
                        }
                        Some(b'f') => {
                            out.push(0x0c);
                            self.pos += 1;
                        }
                        Some(c) => {
                            out.push(c);
                            self.pos += 1;
                        }
                    }
                }
                Some(b'\r') => {
                    out.push(b'\r');
                    self.pos += 1;
                    if self.peek() == Some(b'\n') {
                        out.push(b'\n');
                        self.pos += 1;
                    }
                }
                Some(b'\n') => {
                    out.push(b'\n');
                    self.pos += 1;
                }
                Some(c) => {
                    out.push(c);
                    self.pos += 1;
                }
            }
        }
    }

    /// `<hex...>` string with whitespace-insensitive nibbles.
    fn hex_string(&mut self) -> Result<Vec<u8>, Flow> {
        self.pos += 1;
        let mut out: Vec<u8> = Vec::new();
        let mut high = true;
        loop {
            self.skip_all_ws();
            match self.peek() {
                Some(b'>') => {
                    self.pos += 1;
                    return Ok(out);
                }
                Some(c) => {
                    let Some(v) = hex_value(c) else {
                        return Err(Flow::Stop);
                    };
                    if high {
                        out.push(v << 4);
                    } else {
                        *out.last_mut().unwrap() |= v;
                    }
                    high = !high;
                    self.pos += 1;
                }
                None => return Err(Flow::Stop),
            }
        }
    }

    fn number(&mut self) -> Result<Operand, Flow> {
        let start = self.pos;
        if matches!(self.peek(), Some(b'+') | Some(b'-')) {
            self.pos += 1;
        }
        if self.peek() == Some(b'.') {
            self.pos += 1;
            if !matches!(self.peek(), Some(d) if d.is_ascii_digit()) {
                return Err(Flow::Stop);
            }
        } else {
            if !matches!(self.peek(), Some(d) if d.is_ascii_digit()) {
                return Err(Flow::Stop);
            }
        }
        while matches!(self.peek(), Some(d) if d.is_ascii_digit()) {
            self.pos += 1;
        }
        if self.peek() == Some(b'.') {
            self.pos += 1;
            while matches!(self.peek(), Some(d) if d.is_ascii_digit()) {
                self.pos += 1;
            }
        }
        let slice = &self.data[start..self.pos];
        if slice.contains(&b'.') {
            std::str::from_utf8(slice)
                .ok()
                .and_then(|s| s.parse::<f32>().ok())
                .map(|_| Operand::Real)
                .ok_or(Flow::Stop)
        } else {
            std::str::from_utf8(slice)
                .ok()
                .and_then(|s| s.parse::<i64>().ok())
                .map(Operand::Int)
                .ok_or(Flow::Stop)
        }
    }

    /// `[ ... ]` array of direct objects (references allowed).
    fn array(&mut self, depth: usize) -> Result<Vec<Operand>, Flow> {
        self.pos += 1;
        self.skip_space();
        let mut items = Vec::new();
        loop {
            match self.peek() {
                Some(b']') => {
                    self.pos += 1;
                    return Ok(items);
                }
                Some(_) => items.push(self.direct_object(depth)?),
                None => return Err(Flow::Stop),
            }
        }
    }

    /// A direct object inside an array/dictionary, terminated with `space`;
    /// `depth == 0` is lopdf's `Err::Failure` (too deep).
    fn direct_object(&mut self, depth: usize) -> Result<Operand, Flow> {
        if depth == 0 {
            return Err(Flow::Fail);
        }
        let object = self.object_in(depth - 1)?;
        self.skip_space();
        Ok(object)
    }

    fn object_in(&mut self, depth: usize) -> Result<Operand, Flow> {
        match self.peek() {
            Some(b'(') => Ok(Operand::Str(self.literal_string(MAX_DEPTH)?)),
            Some(b'<') => {
                if self.peek_at(1) == Some(b'<') {
                    self.dictionary(depth)?;
                    Ok(Operand::Dict)
                } else {
                    Ok(Operand::Str(self.hex_string()?))
                }
            }
            Some(b'[') => Ok(Operand::Array(self.array(depth)?)),
            Some(b'/') => Ok(Operand::Name(self.name()?)),
            Some(c) if c.is_ascii_digit() => {
                if self.try_reference() {
                    Ok(Operand::Ref)
                } else {
                    self.number()
                }
            }
            Some(c) if c == b'+' || c == b'-' || c == b'.' => self.number(),
            Some(c) if c.is_ascii_alphabetic() => {
                let start = self.pos;
                while matches!(self.peek(), Some(d) if d.is_ascii_alphabetic()) {
                    self.pos += 1;
                }
                match &self.data[start..self.pos] {
                    b"true" => Ok(Operand::Bool(true)),
                    b"false" => Ok(Operand::Bool(false)),
                    b"null" => Ok(Operand::Null),
                    _ => Err(Flow::Stop),
                }
            }
            _ => Err(Flow::Stop),
        }
    }

    /// lopdf `reference` (`object_id` + `R`), tried before `real`/`integer`.
    fn try_reference(&mut self) -> bool {
        let save = self.pos;
        if !self.try_unsigned() || !self.skip_reference_space() || !self.try_unsigned() {
            self.pos = save;
            return false;
        }
        if !self.skip_reference_space() || self.peek() != Some(b'R') {
            self.pos = save;
            return false;
        }
        self.pos += 1;
        true
    }

    fn try_unsigned(&mut self) -> bool {
        let start = self.pos;
        while matches!(self.peek(), Some(d) if d.is_ascii_digit()) {
            self.pos += 1;
        }
        self.pos > start
    }

    fn skip_reference_space(&mut self) -> bool {
        let start = self.pos;
        self.skip_space();
        self.pos > start
    }

    /// `<< ... >>` dictionary of name/value pairs.
    fn dictionary(&mut self, depth: usize) -> Result<(), Flow> {
        self.pos += 2;
        self.skip_space();
        loop {
            if self.starts_with(b">>") {
                self.pos += 2;
                return Ok(());
            }
            self.name()?;
            self.skip_space();
            self.direct_object(depth)?;
        }
    }

    /// `BI <dict> ID <data> EI`. Any failure is lopdf's `cut` → `Fail`.
    fn inline_image(&mut self) -> Result<(), Flow> {
        self.pos += 2;
        self.skip_ws();

        let mut dict: BTreeMap<Vec<u8>, Operand> = BTreeMap::new();
        loop {
            if self.starts_with(b"ID") {
                break;
            }
            match self.peek() {
                Some(b'/') => {
                    let key = self.name().map_err(|_| Flow::Fail)?;
                    self.skip_space();
                    let value = self.direct_object(MAX_DEPTH).map_err(|_| Flow::Fail)?;
                    dict.insert(key, value);
                }
                _ => return Err(Flow::Fail),
            }
        }
        self.pos += 2;
        self.skip_ws();
        let data_start = self.pos;

        if let Some(data_end) = self.image_data_end(&dict, data_start)
            && self.data.len() >= data_end
        {
            let save = self.pos;
            self.pos = data_end;
            self.skip_ws();
            if self.starts_with(b"EI") {
                self.pos += 2;
                self.skip_ws();
                return Ok(());
            }
            self.pos = save;
        }

        // Fallback equivalent of lopdf's warn-and-skip: locate ` EI ` (space
        // surrounded) so the rest of the content stream can still be parsed.
        self.pos = data_start;
        let rest = &self.data[data_start..];
        let ei_pos = rest
            .windows(4)
            .position(|w| {
                (w[0] == b' ' || w[0] == b'\n' || w[0] == b'\r')
                    && w[1] == b'E'
                    && w[2] == b'I'
                    && (w[3] == b' ' || w[3] == b'\n' || w[3] == b'\r')
            })
            .ok_or(Flow::Fail)?;
        self.pos = data_start + ei_pos + 4;
        self.skip_ws();
        Ok(())
    }

    /// Bound-mirror of `image_data_stream`: computes the byte length of the
    /// inline image data, in bytes from `start`; `None` defers to the EOL-scan.
    fn image_data_end(&self, dict: &BTreeMap<Vec<u8>, Operand>, start: usize) -> Option<usize> {
        let get_abbr = |key: &[u8], long: &[u8]| dict.get(key).or_else(|| dict.get(long));

        if get_abbr(b"F", b"Filter").is_some() {
            return None;
        }

        let width = get_abbr(b"W", b"Width")?.as_int()?;
        let height = get_abbr(b"H", b"Height")?.as_int()?;
        if width < 0 || height < 0 {
            return None;
        }
        let bpc = get_abbr(b"BPC", b"BitsPerComponent")?.as_int()?;
        if bpc < 0 {
            return None;
        }

        let im = get_abbr(b"IM", b"ImageMask").and_then(|x| x.as_bool());
        let num_colors = match im {
            Some(true) => 1,
            _ => {
                let colorspace = get_abbr(b"CS", b"ColorSpace")?.as_name()?;
                match colorspace {
                    b"DeviceGray" | b"Gray" => 1,
                    b"DeviceRGB" | b"RGB" => 3,
                    b"DeviceRGBA" | b"RGBA" => 4,
                    b"DeviceCMYK" | b"CMYK" => 4,
                    _ => return None,
                }
            }
        };

        let stride = (width as usize * (num_colors * bpc as usize)).div_ceil(8);
        let length = height as usize * stride;
        (self.data.len() >= start + length).then_some(start + length)
    }
}

#[inline]
fn hex_value(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'A'..=b'F' => Some(c - b'A' + 10),
        b'a'..=b'f' => Some(c - b'a' + 10),
        _ => None,
    }
}

#[inline]
fn is_regular(c: u8) -> bool {
    !WHITESPACE.contains(&c) && !DELIMITERS.contains(&c)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use lopdf::Encoding;
    use lopdf::Object;
    use lopdf::content::Content;

    use super::extract_page_text;
    use crate::domain::{ParsedPdf, parse_pdf, test_support};

    /// An encoding resolved from real fixture fonts (Helvetica → standard
    /// encoding), so ASCII content decodes to its ASCII text byte-for-byte.
    fn encodings(parsed: &ParsedPdf) -> BTreeMap<Vec<u8>, Encoding<'_>> {
        let document = parsed.document();
        let pages = document.get_pages();
        let page_id = *pages.values().next().expect("one page");
        let fonts = document.get_page_fonts(page_id).expect("fonts");
        let mut map = BTreeMap::new();
        for (name, font) in fonts {
            let encoding = font
                .get_font_encoding_with_limit(document, 1024 * 1024)
                .expect("encoding");
            map.insert(name, encoding);
        }
        map
    }

    /// Reference extractor: lopdf's tokenizer (`Content::decode`) driven by the
    /// exact text-extraction operator handling used by
    /// `extract_text_with_limit` and mirrored by `extract_page_lean`.
    fn reference_extract(content: &[u8], encodings: &BTreeMap<Vec<u8>, Encoding>) -> String {
        let Ok(content) = Content::decode(content) else {
            return String::new();
        };

        let mut text = String::new();
        let mut chunks = String::new();
        let mut current_encoding: Option<&Encoding> = None;
        let mut page_error = false;

        for operation in &content.operations {
            match operation.operator.as_ref() {
                "Tf" => {
                    current_encoding = match operation.operands.first() {
                        Some(operand) => match operand.as_name() {
                            Ok(name) => encodings.get(name),
                            Err(_) => {
                                page_error = true;
                                None
                            }
                        },
                        None => {
                            page_error = true;
                            None
                        }
                    };
                    if !chunks.is_empty() {
                        text.push_str(&chunks);
                        chunks = String::new();
                    }
                }
                "Tj" | "TJ" => {
                    if let Some(encoding) = current_encoding
                        && collect_text(&mut chunks, encoding, &operation.operands).is_err()
                    {
                        page_error = true;
                    }
                }
                "'" => {
                    if let Some(encoding) = current_encoding {
                        if !chunks.ends_with('\n') {
                            chunks.push('\n');
                        }
                        if collect_text(&mut chunks, encoding, &operation.operands).is_err() {
                            page_error = true;
                        }
                    }
                }
                "\"" => {
                    if let Some(encoding) = current_encoding {
                        if !chunks.ends_with('\n') {
                            chunks.push('\n');
                        }
                        if let Some(string_operand) = operation.operands.get(2)
                            && collect_text(
                                &mut chunks,
                                encoding,
                                std::slice::from_ref(string_operand),
                            )
                            .is_err()
                        {
                            page_error = true;
                        }
                    }
                }
                "T*" if !chunks.ends_with('\n') => chunks.push('\n'),
                "T*" => {}
                "ET" if !chunks.ends_with('\n') => chunks.push('\n'),
                "ET" => {}
                _ => {}
            }
        }
        if !chunks.is_empty() {
            text.push_str(&chunks);
        }

        if page_error { String::new() } else { text }
    }

    fn collect_text(text: &mut String, encoding: &Encoding, operands: &[Object]) -> Result<(), ()> {
        for operand in operands {
            match operand {
                Object::String(bytes, _) => {
                    encoding.write_to_string(bytes, text).map_err(|_| ())?
                }
                Object::Array(array) => {
                    collect_text(text, encoding, array)?;
                    text.push(' ');
                }
                Object::Integer(value) if *value < -100 => text.push(' '),
                _ => {}
            }
        }
        Ok(())
    }

    fn parsed() -> ParsedPdf {
        parse_pdf(test_support::valid_pdf(1).as_bytes()).unwrap()
    }

    #[test]
    fn hand_computed_extractions() {
        let parsed = parsed();
        let encs = encodings(&parsed);
        // No `Tf` has selected a font encoding yet, so nothing is decoded.
        assert_eq!(extract_page_text(b"(Hello World) Tj", &encs), "");
        assert_eq!(
            extract_page_text(b"BT /F1 12 Tf (Hello World) Tj ET", &encs),
            "Hello World\n"
        );
        assert_eq!(
            extract_page_text(b"BT /F1 12 Tf (A) Tj (B) Tj ET", &encs),
            "AB\n"
        );
        assert_eq!(
            extract_page_text(b"BT /F1 12 Tf (a) Tj (b) ' ET", &encs),
            "a\nb\n"
        );
        // `T*` ignores its operands (no text is collected for it), so the
        // `(b)` string is discarded; only its newline survives.
        assert_eq!(
            extract_page_text(b"BT /F1 12 Tf (a) Tj (b) T* (c) ET", &encs),
            "a\n"
        );
        assert_eq!(
            extract_page_text(b"BT /F1 12 Tf [(a) -150 (b)] TJ ET", &encs),
            "a b \n"
        );
        // `"` takes `[aw ac string]`; operand 2 is the shown string.
        assert_eq!(
            extract_page_text(b"BT /F1 12 Tf (a) Tj 8 8 (c) \" ET", &encs),
            "a\nc\n"
        );
    }

    #[test]
    fn scanner_matches_lopdf_tokenizer_across_grammar_patterns() {
        let parsed = parsed();
        let encs = encodings(&parsed);
        let patterns: &[(&[u8], &str)] = &[
            (b"(Hello World) Tj", "plain Tj"),
            (b"(A) Tj (B) Tj", "two Tj no interstitial ws output"),
            (b"(a) Tj (b) '", "apostrophe operator"),
            (b"(a) Tj (b) \" 1 2 (c)", "quote operator"),
            (b"[(a) -150 (b)] TJ", "TJ negative advance"),
            (b"[(hello) 12 (cjk) -300] TJ", "TJ mixed numbers"),
            (b"<416263> Tj", "hex string"),
            (b"<61 62 63> Tj", "hex spaced"),
            (b"(esc\\141p\\101) Tj", "octal escapes"),
            (b"(t\\tab\\nnewline) Tj", "named escapes"),
            (b"(a\\)b) Tj", "escaped paren"),
            (b"(back\\\\slash) Tj", "escaped backslash"),
            (b"(paren(out)side) Tj", "nested parens"),
            (b"BT /F1 12 Tf 72 720 Td (Page) Tj ET", "valid text block"),
            (b"/F1#20x 12 Tf (x) Tj", "name hex escape"),
            (
                b"BT /F1 12 Tf 72 720 Td (Page) Tj ET\nBT /F2 10 Tf 72 700 Td (Two) Tj ET",
                "chunk flush",
            ),
            (
                b"% lead comment\nBT /F1 12 Tf (F1) Tj ET",
                "leading comment",
            ),
            (b"(a) Tj % between\n(b) Tj", "comment between ops"),
            (b"(a) % mid\n(b) Tj", "comment mid-op (lopdf drops)"),
            (b"q 0 0 0 rg 12 34 56 78 re f Q", "graphics ops"),
            (b"BS /w 0 /s /S 0 0 m 100 100 l S", "path ops"),
            (b".5 1.5 -3 12 5. m", "number forms"),
            (b"(a) Tj @@@", "garbage after op"),
            (b"(a", "unterminated string"),
            (b"BT /Nope 12 Tf (x) Tj ET", "unknown font / silent skip"),
            (b"<< /x (a) /y [(b)] >> 0 0 m", "dictionary operand"),
            (b"(line1\r\nline2) Tj", "eol kept verbatim"),
            (b"(line following\nline) Tj", "raw newline inside string"),
            (b"<ff> Tj", "single nibble hex"),
            (b"<> Tj", "empty hex"),
            (b"(a) Tj\n(b) T* (c) ET", "T* and ET newlines"),
            (b"[(a) 55 (b)] TJ", "TJ positive gaps"),
            (b"(a) Tj (b) Tj ' (c)", "mixed operators"),
            (
                b"BI /W 1 /H 1 /BPC 8 /CS /RGB ID \x01\x02\x03\n(after) Tj",
                "inline image then text",
            ),
            (
                b"BI /W 1 /H 1 /BPC 8 ID \x01\x02\x03 EI (after) Tj",
                "inline image no EI ws",
            ),
        ];

        for (content, desc) in patterns {
            let scanner = extract_page_text(content, &encs);
            let reference = reference_extract(content, &encs);
            assert_eq!(scanner, reference, "pattern diverged: {desc}");
        }
    }
}
