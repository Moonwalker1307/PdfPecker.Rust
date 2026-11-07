#!/usr/bin/env python3
"""Writes wheels.txt: for each of the eight platforms, the pdfpecker wheel on PyPI that holds its native library.

    python3 scripts/update-wheels.py            the version in Cargo.toml

The build script downloads only the wheel for the platform it builds for, and checks it and the library inside it
against the SHA-256 values written here, so a build never trusts anything it downloads. Run this after changing the
version in Cargo.toml and commit the result; never edit wheels.txt by hand.
"""
import hashlib
import io
import json
import pathlib
import re
import sys
import urllib.request
import zipfile

ROOT = pathlib.Path(__file__).resolve().parent.parent

# The platform keys match the npm package's, so the two packages talk about platforms the same way.
PLATFORMS = [
    ("linux-x64", r"-manylinux_[0-9_]+_x86_64\.whl$", "pdfpecker_native.so"),
    ("linux-arm64", r"-manylinux_[0-9_]+_aarch64\.whl$", "pdfpecker_native.so"),
    ("linux-x64-musl", r"-musllinux_[0-9_]+_x86_64\.whl$", "pdfpecker_native.so"),
    ("linux-arm64-musl", r"-musllinux_[0-9_]+_aarch64\.whl$", "pdfpecker_native.so"),
    ("darwin-arm64", r"-macosx_[0-9_]+_arm64\.whl$", "pdfpecker_native.dylib"),
    ("darwin-x64", r"-macosx_[0-9_]+_x86_64\.whl$", "pdfpecker_native.dylib"),
    ("win32-x64", r"-win_amd64\.whl$", "pdfpecker_native.dll"),
    ("win32-arm64", r"-win_arm64\.whl$", "pdfpecker_native.dll"),
]


def main() -> int:
    if len(sys.argv) != 1:
        print("usage: update-wheels.py", file=sys.stderr)
        return 2
    version = re.search(r'^version = "([^"]+)"', (ROOT / "Cargo.toml").read_text(encoding="utf-8"), re.M).group(1)
    with urllib.request.urlopen(f"https://pypi.org/pypi/pdfpecker/{version}/json") as response:
        files = [f for f in json.load(response)["urls"] if f["packagetype"] == "bdist_wheel"]
    lines = [
        f"# The pdfpecker {version} wheels on PyPI, written by scripts/update-wheels.py; do not edit.",
        "# platform wheel-url wheel-sha256 library library-sha256",
    ]
    for key, pattern, library in PLATFORMS:
        wheels = [f for f in files if re.search(pattern, f["filename"])]
        if len(wheels) != 1:
            print(f"{key}: expected one wheel, PyPI lists {len(wheels)}", file=sys.stderr)
            return 1
        wheel = wheels[0]
        with urllib.request.urlopen(wheel["url"]) as response:
            data = response.read()
        if hashlib.sha256(data).hexdigest() != wheel["digests"]["sha256"]:
            print(f"{wheel['filename']}: the download does not match PyPI's SHA-256", file=sys.stderr)
            return 1
        lib = zipfile.ZipFile(io.BytesIO(data)).read(f"pdfpecker/{library}")
        lines.append(f"{key} {wheel['url']} {wheel['digests']['sha256']} {library} {hashlib.sha256(lib).hexdigest()}")
        print(f"{key}: {wheel['filename']} ({len(lib):,} byte library)")
    (ROOT / "wheels.txt").write_text("\n".join(lines) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    sys.exit(main())
