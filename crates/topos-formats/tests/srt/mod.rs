use topos_formats::{SearchFormat, srt::SRTLocation};
use topos_lib::{error::AnyResult, matcher::BibleMatcher};

/// A real transcript (with multi-line cues): every match must be located in a cue
#[test]
fn tdp_c1() -> AnyResult<()> {
    let srt = include_str!("./Chapter 1 - The Command of Christ transcript.srt");
    let matcher = BibleMatcher::default();
    let results = matcher.search_format::<SRTLocation>(srt)?;
    assert!(!results.is_empty());
    assert!(results.iter().all(|m| m.location.id > 0));
    assert!(results.iter().all(|m| m.location.start <= m.location.end));
    Ok(())
}
