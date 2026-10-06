use pyo3::prelude::*;
use topos_lib::matcher::{location::line_col::LineColLocation, matcher::BibleMatcher};

/// Formats the sum of two numbers as string.
#[pyfunction]
fn sum_as_string(a: usize, b: usize) -> PyResult<String> {
    Ok((a + b).to_string())
}

#[pyfunction]
fn search(input: &str) -> PyResult<String> {
    let m = BibleMatcher::default();
    let result = m.search::<LineColLocation>(input).unwrap_or_default();
    Ok(result
        .iter()
        .map(|r| {
            let name = m.data().books().get_name(r.psg.book).unwrap();
            let segments = r.psg.segments.to_string();
            format!("{name} {segments}")
        })
        .collect::<Vec<_>>()
        .join(" | "))
}

/// A Python module implemented in Rust.
#[pymodule]
fn topos(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(sum_as_string, m)?)?;
    m.add_function(wrap_pyfunction!(search, m)?)?;
    Ok(())
}
