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
print(page.markdown())
print(page.select_markdown("main"))
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

Onion-service HTTP pages are fetched through [`onionlink`](https://github.com/RustedBytes/onionlink-rs) automatically when the URL host ends in `.onion`:

```python
import servofetch

browser = servofetch.Browser(timeout=30.0)
page = browser.fetch("http://archiveiya74codqgiixo33q62qlrqtkgmcitqx5u2oeqnmn5bpcbiyd.onion/")
print(page.text())
```

The onion backend supports raw `http://` onion content for `go`, `fetch`, `markdown`, `text`, and `extract_json`. JavaScript evaluation and screenshots are not supported for onion URLs because this path does not render through Servo.

## API

- `Browser.go(url, ...) -> Page`
- `Browser.fetch(url, ...) -> Page`
- `Browser.markdown(url, ..., selector=None, javascript=None) -> str`
- `Browser.text(url, ..., javascript=None) -> str`
- `Browser.extract_json(url, ..., selector=None, javascript=None) -> str`
- `Browser.screenshot(url, ..., filename=None) -> Page`
- `Browser.map(url, ..., include=None, exclude=None) -> list[MappedUrl]`
- `Browser.crawl(url, ..., selector=None, json=False) -> list[CrawlResult]`
- `AsyncBrowser` provides awaitable versions of the same methods.

Returned `Page` objects keep their source URL, so `page.markdown()` and `page.extract_json()` resolve relative links against the fetched page by default. Pass `url="..."` only when you need to override that base URL. Use `page.text()`, `page.select_markdown(selector)`, or `page.select_json(selector)` to extract from an already-fetched page. Page metadata includes `html_len`, `text_len`, `has_layout`, `has_accessibility_tree`, `console_error_count`, `has_screenshot`, and `screenshot_len`.

Pass `filename="page.png"` to `screenshot` to write the captured PNG while still receiving the returned `Page`. A screenshot `Page` also exposes `page.screenshot_png`, `page.has_screenshot`, and `page.save_screenshot(filename)`.

Private and local network addresses are blocked by default. Pass `allow_private_addresses=True` when constructing the first browser instance to allow them for the current process.

Onion options can be set on `Browser` or `AsyncBrowser`: `onion_bootstrap`, `onion_consensus_file`, `onion_verbose`, and `onion_response_limit`. The default bootstrap endpoint is `128.31.0.39:9131`, matching onionlink.
