use topos_lib::{
    error::AnyResult,
    matcher::{
        location::{html::HTMLLocation, srt::SRTLocation},
        matcher::BibleMatcher,
    },
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
fn tdp_html() -> AnyResult<()> {
    // let html = include_str!("./The Dorean Principle.html");
    let html = include_str!(
        "/home/dgmastertemple/Downloads/tdp/The Dorean Principle - A Biblical Response to the Commercialization of Christianity.html"
    );
    let matcher = BibleMatcher::default();
    let results = matcher.search::<HTMLLocation>(html)?;
    dbg!(results);
    Ok(())
}
