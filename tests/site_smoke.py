#!/usr/bin/env python3
"""Validate the versioned adoption surface and optionally check public links."""

import argparse
from html.parser import HTMLParser
import json
from pathlib import Path
from urllib.error import HTTPError, URLError
from urllib.parse import urlparse
from urllib.request import Request, urlopen
import xml.etree.ElementTree as ET


ROOT = Path(__file__).resolve().parents[1]
SITE = ROOT / "www"


class SurfaceParser(HTMLParser):
    def __init__(self) -> None:
        super().__init__()
        self.ids: set[str] = set()
        self.references: list[tuple[str, str]] = []

    def handle_starttag(
        self, tag: str, attrs: list[tuple[str, str | None]]
    ) -> None:
        values = dict(attrs)
        if element_id := values.get("id"):
            self.ids.add(element_id)
        for attribute in ("href", "src"):
            if value := values.get(attribute):
                self.references.append((attribute, value))


def check_public_link(url: str) -> None:
    parsed = urlparse(url)
    probe_url = url
    if parsed.netloc == "crates.io" and parsed.path.startswith("/crates/"):
        crate_name = parsed.path.removeprefix("/crates/").split("/", maxsplit=1)[0]
        probe_url = f"https://crates.io/api/v1/crates/{crate_name}"

    request = Request(probe_url, headers={"User-Agent": "soon-site-smoke/1.0"})
    try:
        with urlopen(request, timeout=20) as response:
            assert response.status < 400, (
                f"public link returned {response.status}: {url}"
            )
    except HTTPError as error:
        raise AssertionError(
            f"public link returned {error.code}: {url}"
        ) from error
    except URLError as error:
        raise AssertionError(f"public link failed: {url}: {error.reason}") from error


def main() -> None:
    argument_parser = argparse.ArgumentParser()
    argument_parser.add_argument(
        "--online",
        action="store_true",
        help="request every external link after the offline checks pass",
    )
    args = argument_parser.parse_args()

    index_path = SITE / "index.html"
    html = index_path.read_text(encoding="utf-8")
    parser = SurfaceParser()
    parser.feed(html)
    parser.close()
    public_links: set[str] = set()

    assert parser.get_starttag_text() is not None, "site/index.html is empty"
    assert "cargo install soon" in html
    assert "python -m pip install soon-bin" in html
    assert 'eval "$(soon init zsh)"' in html
    assert "v0.5.0" in html
    assert "No automatic execution" in html

    for attribute, reference in parser.references:
        parsed = urlparse(reference)
        if parsed.scheme:
            assert parsed.scheme == "https", f"{attribute} must use HTTPS: {reference}"
            public_links.add(reference)
            continue
        if reference.startswith("#"):
            assert reference[1:] in parser.ids, f"missing anchor: {reference}"
            continue

        path = parsed.path
        if path == "/":
            continue
        target = SITE / path.lstrip("/")
        assert target.is_file(), f"missing local asset: {reference}"

    demo_path = SITE / "assets" / "soon-demo.svg"
    ET.parse(demo_path)
    demo = demo_path.read_text(encoding="utf-8")
    for phrase in ("git psuh", "git push", "Ctrl-F", "REPAIR", "Nothing executes"):
        assert phrase in demo, f"demo is missing product beat: {phrase}"

    headers = (SITE / "_headers").read_text(encoding="utf-8")
    for header in (
        "Content-Security-Policy",
        "Permissions-Policy",
        "Referrer-Policy",
        "X-Content-Type-Options",
    ):
        assert header in headers, f"missing security header: {header}"

    wrangler = json.loads((ROOT / "wrangler.jsonc").read_text(encoding="utf-8"))
    assert wrangler["assets"]["directory"] == "./www"
    assert wrangler["routes"] == [
        {"pattern": "soon.hydroroll.team", "custom_domain": True}
    ]

    if args.online:
        for link in sorted(public_links):
            check_public_link(link)

    print(
        f"site smoke passed: {len(parser.references)} links/assets, "
        f"{len(parser.ids)} anchors, {len(public_links)} public links"
    )


if __name__ == "__main__":
    main()
