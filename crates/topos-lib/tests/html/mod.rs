use topos_lib::{
    error::AnyResult,
    matcher::{bible_matcher::BibleMatcher, location::html::HTMLLocation},
};

/**
```js
function goToTextFragment(fragment) {
    const button = document.createElement("a");
    button.href = fragment;
    button.click();
    button.remove();
}
goToTextFragment("#:~:text=Rom.%2016:23")
```
*/
#[test]
#[ignore = "htmloc fragment generation takes minutes on this 3 MB fixture; run with --ignored"]
fn tdp_html() -> AnyResult<()> {
    let html = include_str!("./The Dorean Principle.html");
    let matcher = BibleMatcher::default();
    let results = matcher.search::<HTMLLocation>(html)?;
    dbg!(results);
    Ok(())
}
