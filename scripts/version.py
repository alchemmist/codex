import argparse
from pathlib import Path
import re
import tomllib


ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "antex-rs/Cargo.toml"
LOCK = ROOT / "antex-rs/Cargo.lock"


def current_version():
    return tomllib.loads(MANIFEST.read_text())["workspace"]["package"]["version"]


def bump(version, level):
    parts = [int(part) for part in version.split(".")]
    if len(parts) != 3:
        raise ValueError("workspace version must be SemVer")
    index = {"major": 0, "minor": 1, "patch": 2}[level]
    parts[index] += 1
    for later in range(index + 1, 3):
        parts[later] = 0
    return ".".join(str(part) for part in parts)


def replace_manifest(text, old, new):
    marker = f'version = "{old}"'
    if text.count(marker) != 1:
        raise ValueError("canonical workspace version is ambiguous")
    return text.replace(marker, f'version = "{new}"', 1)


def replace_lock(text, old, new):
    package = re.compile(
        rf'(?ms)(\[\[package\]\]\nname = "antex-[^"]+"\nversion = "){re.escape(old)}("\n)'
    )
    updated, count = package.subn(rf"\g<1>{new}\2", text)
    if count != 7:
        raise ValueError(f"expected 7 internal lockfile packages, found {count}")
    return updated


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("level", choices=["major", "minor", "patch"])
    parser.add_argument("--print", action="store_true", dest="print_only")
    args = parser.parse_args()
    old = current_version()
    new = bump(old, args.level)
    if not args.print_only:
        MANIFEST.write_text(replace_manifest(MANIFEST.read_text(), old, new))
        LOCK.write_text(replace_lock(LOCK.read_text(), old, new))
    print(new)


if __name__ == "__main__":
    main()
