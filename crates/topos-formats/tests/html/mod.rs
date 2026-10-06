/*!
For debugging/opening Text Fragments:

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
