#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def replace(path: str, old: str, new: str) -> None:
    target = ROOT / path
    text = target.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"expected one match in {path}, found {count}: {old!r}")
    target.write_text(text.replace(old, new), encoding="utf-8")


replace(
    "README.md",
    "- `api.zpkg.net` is the package metadata and control-plane API;\n"
    "- `registry.zpkg.net` serves immutable artifacts;",
    "- `api.zpkg.net` owns authenticated package and control-plane writes;\n"
    "- `registry.zpkg.net` serves public package and version metadata reads;\n"
    "- `cdn.zpkg.net` serves immutable artifact bytes;",
)

replace(
    "src/pages/index.astro",
    "        lean, tag-verified package metadata through\n"
    "        <code>api.zpkg.net</code>; immutable artifacts are downloaded from\n"
    "        <a href=\"https://registry.zpkg.net\">registry.zpkg.net</a>. Your backing",
    "        lean, tag-verified package metadata through\n"
    "        <code>api.zpkg.net</code>; public version metadata is read from\n"
    "        <a href=\"https://registry.zpkg.net\">registry.zpkg.net</a>; and immutable\n"
    "        artifact bytes are downloaded from\n"
    "        <a href=\"https://cdn.zpkg.net\">cdn.zpkg.net</a>. Your backing",
)
replace(
    "src/pages/index.astro",
    "  <span class=\"c\">artifact -&gt; https://registry.zpkg.net</span>",
    "  <span class=\"c\">artifact -&gt; https://cdn.zpkg.net</span>",
)
replace(
    "src/pages/index.astro",
    "            anchors the source; the registry serves the immutable bytes.",
    "            anchors the source; the registry serves metadata and the CDN serves immutable bytes.",
)
replace(
    "src/pages/index.astro",
    "        <pre>GET https://registry.zpkg.net/v1/files/acme/ui-kit/1.2.0/dist/style.css",
    "        <pre>GET https://cdn.zpkg.net/v1/files/acme/ui-kit/1.2.0/dist/style.css",
)
replace(
    "src/pages/index.astro",
    "          registry.zpkg.net is the public artifact host; the repo you declare in\n"
    "          <code>.zpkg.toml</code> is the mirror, the backup, and the\n"
    "          provenance anchor.",
    "          registry.zpkg.net is the public metadata host and cdn.zpkg.net serves\n"
    "          immutable package bytes; the repo you declare in\n"
    "          <code>.zpkg.toml</code> remains the mirror, backup, and provenance anchor.",
)
replace(
    "tests/site_contract.py",
    "    assert \"registry.zpkg.net\" in body\n",
    "    assert \"registry.zpkg.net\" in body\n    assert \"cdn.zpkg.net\" in body\n",
)

print("canonical zpkg.net service roles corrected")
