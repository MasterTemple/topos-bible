use std::ops::Range;

use topos_lib::matcher::{BibleMatch, BibleMatcher};

use crate::{Format, FormatError};

/// Where a match is in a subtitle file (SubRip `.srt`, WebVTT `.vtt`, or SubViewer `.sbv`)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SRTLocation {
    /// The id of the cue the match starts in
    pub id: u32,
    /// When the cue the match starts in begins
    pub start: SRTTimeStamp,
    /// When the cue the match ends in ends
    pub end: SRTTimeStamp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SRTTimeStamp {
    pub hours: u32,
    pub minutes: u32,
    pub seconds: u32,
    pub millis: u32,
}

impl SRTTimeStamp {
    /// `00:22:57,920`, `00:22:57.920` (WebVTT and SBV), or `22:57.920` (WebVTT, without hours)
    pub fn parse(s: &str) -> Option<Self> {
        let (hms, millis) = s.trim().split_once([',', '.'])?;
        let parts: Vec<u32> = hms
            .split(':')
            .map(|n| n.trim().parse().ok())
            .collect::<Option<_>>()?;
        let (hours, minutes, seconds) = match parts[..] {
            [hours, minutes, seconds] => (hours, minutes, seconds),
            [minutes, seconds] => (0, minutes, seconds),
            _ => return None,
        };
        Some(Self {
            hours,
            minutes,
            seconds,
            millis: millis.trim().parse().ok()?,
        })
    }
}

/// One subtitle: an optional id, a time range, and one or more lines of text
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SRTCue<'a> {
    pub id: u32,
    pub start: SRTTimeStamp,
    pub end: SRTTimeStamp,
    pub text: &'a str,
    /// Byte range of the whole cue in the file
    pub span: Range<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SRTDocument<'a> {
    pub cues: Vec<SRTCue<'a>>,
}

impl<'a> SRTDocument<'a> {
    /// Parses cues separated by blank lines; blocks that are not cues are skipped
    pub fn parse(input: &'a str) -> Self {
        let mut cues = vec![];
        let mut block_start = None;
        let mut offset = 0;
        for line in input.split_inclusive('\n') {
            let blank = line.trim().trim_start_matches('\u{FEFF}').is_empty();
            match (blank, block_start) {
                (false, None) => block_start = Some(offset),
                (true, Some(start)) => {
                    cues.extend(Self::parse_cue(input, start..offset));
                    block_start = None;
                }
                _ => {}
            }
            offset += line.len();
        }
        if let Some(start) = block_start {
            cues.extend(Self::parse_cue(input, start..offset));
        }
        Self { cues }
    }

    fn parse_cue(input: &'a str, span: Range<usize>) -> Option<SRTCue<'a>> {
        let block = &input[span.clone()];
        let mut lines = block.split_inclusive('\n');
        let first = lines.next()?;
        // The id line is optional, and in WebVTT it may be text (only numbers are kept)
        let (id, timing, (start, end)) = match Self::parse_timing(first) {
            Some(times) => (0, first, times),
            None => {
                let timing = lines.next()?;
                let id = first
                    .trim()
                    .trim_start_matches('\u{FEFF}')
                    .parse()
                    .unwrap_or(0);
                (id, timing, Self::parse_timing(timing)?)
            }
        };
        let text_start = (timing.as_ptr() as usize - block.as_ptr() as usize) + timing.len();
        Some(SRTCue {
            id,
            start,
            end,
            text: block[text_start..].trim_end(),
            span,
        })
    }

    /**
    - SRT and WebVTT: `00:00:01,000 --> 00:00:02,000`, where WebVTT may add cue settings after
      the end (`align:start`)
    - SBV: `0:00:01.000,0:00:02.000`
    */
    fn parse_timing(line: &str) -> Option<(SRTTimeStamp, SRTTimeStamp)> {
        let (start, end) = match line.split_once("-->") {
            Some((start, end)) => (start, end.split_whitespace().next()?),
            None => line.trim().split_once(',')?,
        };
        Some((SRTTimeStamp::parse(start)?, SRTTimeStamp::parse(end)?))
    }

    /// The cue that contains this byte offset
    pub fn cue_at(&self, byte: usize) -> Option<&SRTCue<'a>> {
        let idx = self.cues.partition_point(|cue| cue.span.end <= byte);
        self.cues.get(idx).filter(|cue| cue.span.contains(&byte))
    }
}

impl Format for SRTLocation {
    type Input<'a> = &'a str;

    /// - The whole file is searched, so references that continue into the next cue are found
    /// - Note that a blank line ends a reference, and cues are separated by blank lines
    fn search(
        matcher: &BibleMatcher,
        input: Self::Input<'_>,
    ) -> Result<Vec<BibleMatch<Self>>, FormatError> {
        let doc = SRTDocument::parse(input);
        matcher
            .search(input)
            .into_iter()
            .map(|m| {
                let bytes = m.location.bytes;
                let first = doc
                    .cue_at(bytes.start)
                    .ok_or(SRTMatchError::OutsideCue(bytes.start))?;
                let last = doc.cue_at(bytes.end.saturating_sub(1)).unwrap_or(first);
                let location = SRTLocation {
                    id: first.id,
                    start: first.start,
                    end: last.end,
                };
                Ok(m.map_loc(|_| location))
            })
            .collect()
    }
}

#[derive(thiserror::Error, Debug)]
pub enum SRTMatchError {
    #[error("match at byte {0} is not inside a subtitle cue")]
    OutsideCue(usize),
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRT: &str = "\u{FEFF}1\r\n00:00:01,000 --> 00:00:02,500\r\nRead John 3:16\r\nand Romans 8:28.\r\n\r\n2\r\n00:01:00.000 --> 00:01:03,000\r\nNo references here\r\n";

    #[test]
    fn parses_multi_line_cues() {
        let doc = SRTDocument::parse(SRT);
        assert_eq!(doc.cues.len(), 2);
        assert_eq!(doc.cues[0].id, 1);
        assert_eq!(doc.cues[0].text, "Read John 3:16\r\nand Romans 8:28.");
        assert_eq!(doc.cues[1].start.minutes, 1);
        assert_eq!(doc.cues[1].text, "No references here");
    }

    #[test]
    fn skips_blocks_that_are_not_cues() {
        let doc = SRTDocument::parse("WEBVTT\n\n00:00:01,000 --> 00:00:02,000\nJohn 1:1\n");
        assert_eq!(doc.cues.len(), 1);
        assert_eq!(doc.cues[0].id, 0);
    }

    #[test]
    fn parses_webvtt_and_sbv() {
        let vtt = "WEBVTT\n\nintro\n00:01.000 --> 00:04.250 align:start\nSee John 3:16\n";
        let doc = SRTDocument::parse(vtt);
        assert_eq!(doc.cues.len(), 1);
        assert_eq!(doc.cues[0].end.millis, 250);
        assert_eq!(doc.cues[0].text, "See John 3:16");

        let sbv = "0:00:01.000,0:00:03.500\nSee John 3:16\n\n0:00:04.000,0:00:05.000\nand more\n";
        let doc = SRTDocument::parse(sbv);
        assert_eq!(doc.cues.len(), 2);
        assert_eq!(doc.cues[0].end.seconds, 3);
        assert_eq!(doc.cues[1].text, "and more");
    }

    #[test]
    fn locates_matches() {
        let matcher = BibleMatcher::default();
        let matches = SRTLocation::search(&matcher, SRT).unwrap();
        let ids: Vec<_> = matches.iter().map(|m| m.location.id).collect();
        assert_eq!(ids, [1, 1]);
        assert_eq!(matches[0].location.end.millis, 500);
    }
}
