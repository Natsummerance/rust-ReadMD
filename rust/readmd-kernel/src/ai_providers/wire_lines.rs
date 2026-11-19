//! Read live provider lines without waiting for the byte after a CR.
//! The SSE grammar accepts CR, LF and CRLF, including across socket reads.
use std::io::BufRead;

pub(crate) struct WireLines<R> {
    reader: R,
    remaining: usize,
    after_cr: bool,
    first: bool,
}

impl<R: BufRead> WireLines<R> {
    pub(crate) fn new(reader: R, budget: usize) -> Self {
        Self {
            reader,
            remaining: budget,
            after_cr: false,
            first: true,
        }
    }

    pub(crate) fn next(&mut self, limit: usize) -> Result<Option<String>, &'static str> {
        let mut line = Vec::new();
        loop {
            let chunk = self.reader.fill_buf().map_err(|_| "ai_network_error")?;
            if chunk.is_empty() {
                if line.is_empty() {
                    return Ok(None);
                }
                break;
            }
            if std::mem::take(&mut self.after_cr) && chunk[0] == b'\n' {
                self.remaining = self.remaining.checked_sub(1).ok_or("review_ai_budget")?;
                self.reader.consume(1);
                continue;
            }
            let end = memchr::memchr2(b'\r', b'\n', chunk);
            let used = end.map_or(chunk.len(), |i| i + 1);
            self.remaining = self.remaining.checked_sub(used).ok_or("review_ai_budget")?;
            if used > limit.saturating_sub(line.len()) {
                return Err("review_ai_budget");
            }
            line.extend_from_slice(&chunk[..end.unwrap_or(used)]);
            if let Some(i) = end {
                self.after_cr = chunk[i] == b'\r';
                line.push(b'\n');
            }
            self.reader.consume(used);
            if end.is_some() {
                break;
            }
        }
        if std::mem::take(&mut self.first) && line.starts_with(b"\xef\xbb\xbf") {
            line.drain(..3);
        }
        String::from_utf8(line)
            .map(Some)
            .map_err(|_| "ai_invalid_response")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufReader;

    #[test]
    fn split_delimiters_bom_unicode_and_full_wire_budget() {
        let input = "\u{feff}中\r\n\rabc\n\u{feff}尾".as_bytes();
        for size in 1..=input.len() {
            let mut lines = WireLines::new(BufReader::with_capacity(size, input), input.len());
            for expected in ["中\n", "\n", "abc\n", "\u{feff}尾"] {
                assert_eq!(lines.next(80).unwrap().as_deref(), Some(expected));
            }
            assert_eq!(lines.next(80), Ok(None));
        }
        let mut lines = WireLines::new(&b"x\r\n"[..], 2);
        assert_eq!(lines.next(2).unwrap().as_deref(), Some("x\n"));
        assert_eq!(lines.next(2), Err("review_ai_budget"));
        assert_eq!(
            WireLines::new(&b"abcd"[..], 10).next(3),
            Err("review_ai_budget")
        );
        assert_eq!(
            WireLines::new(&b"\xff\n"[..], 10).next(3),
            Err("ai_invalid_response")
        );
    }
}
