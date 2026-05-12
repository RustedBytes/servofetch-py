# servofetch

Python 3.10+ bindings for [`servo-fetch`](https://github.com/konippi/servo-fetch), built with PyO3 and maturin.

`servofetch` embeds Servo through the Rust crate and exposes a small Python API for fetching rendered pages, extracting content, running JavaScript, and taking screenshots.

## Install for Development

```bash
python -m pip install maturin
maturin develop
```

## Usage

```python
import servofetch

browser = servofetch.Browser(timeout=30.0, settle_ms=500)

page = browser.go("https://example.com")
print(page.title)
print(page.markdown("https://example.com"))
```

```python
import asyncio
import servofetch

async def main():
    browser = servofetch.AsyncBrowser(timeout=30.0)
    text = await browser.text("https://example.com")
    print(text)

asyncio.run(main())
```

## API

- `Browser.go(url, ...) -> Page`
- `Browser.markdown(url, ...) -> str`
- `Browser.text(url, ...) -> str`
- `Browser.extract_json(url, ...) -> str`
- `Browser.screenshot(url, ..., filename=None) -> Page`
- `AsyncBrowser` provides awaitable versions of the same methods.

Pass `filename="page.png"` to `screenshot` to write the captured PNG while still receiving the returned `Page`.

Private and local network addresses are blocked by default. Pass `allow_private_addresses=True` when constructing the first browser instance to allow them for the current process.
