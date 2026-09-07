import argparse
import hashlib
import json
from pathlib import Path
import platform
import subprocess
import sys
import time


ROOT = Path(__file__).resolve().parents[1]
TARGETS = ["aarch64-apple-darwin", "x86_64-unknown-linux-gnu"]


def run(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def production_graph(metadata, root_name):
    packages = {package["id"]: package for package in metadata["packages"]}
    root_names = [root_name] if isinstance(root_name, str) else root_name
    roots = [key for key, package in packages.items() if package["name"] in root_names]
    if len(roots) != len(root_names):
        raise ValueError(f"expected exactly one root package for each of {root_names}")
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    pending = roots[:]
    reached = set()
    reverse = {}
    while pending:
        key = pending.pop()
        if key in reached:
            continue
        reached.add(key)
        for dependency in nodes[key]["deps"]:
            if not any(kind["kind"] != "dev" for kind in dependency["dep_kinds"]):
                continue
            target = dependency["pkg"]
            reverse.setdefault(target, set()).add(key)
            pending.append(target)
    return packages, reached, reverse


def inventory(metadata, root_name, forbidden):
    packages, reached, reverse = production_graph(metadata, root_name)
    members = set(metadata["workspace_members"])
    local = reached & members
    files = {}
    for key in sorted(local):
        package = packages[key]
        source = Path(package["manifest_path"]).parent / "src"
        for path in sorted(source.rglob("*.rs")):
            if (
                "tests" in path.parts
                or path.stem == "tests"
                or path.stem.endswith("_tests")
            ):
                continue
            files[str(path.relative_to(ROOT))] = len(path.read_text().splitlines())
    return {
        "workspace_crates": sorted(packages[key]["name"] for key in members),
        "production_workspace_crates": sorted(packages[key]["name"] for key in local),
        "dependency_nodes": len(reached),
        "dependency_scope": "resolved normal and build edges, all targets; dev edges excluded",
        "production_source_lines_upper_bound": sum(files.values()),
        "source_count_method": "src/**/*.rs excluding dedicated tests; includes inline tests and comments",
        "modules_over_800_lines": {
            path: lines for path, lines in files.items() if lines > 800
        },
        "forbidden_dependencies": sorted(
            {packages[key]["name"] for key in reached} & set(forbidden)
        ),
        "reverse_dependencies": {
            key: sorted(reverse.get(key, [])) for key in sorted(reached)
        },
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    workspace = ROOT / "antex-rs"
    metadata = json.loads(
        run(
            "cargo",
            "metadata",
            "--locked",
            "--offline",
            "--format-version",
            "1",
            "--manifest-path",
            str(workspace / "Cargo.toml"),
        )
    )
    root_name = ["antex-cli"]
    forbidden = json.loads((ROOT / "migration/forbidden-dependencies.json").read_text())
    report = inventory(metadata, root_name, forbidden)
    report.update(
        {
            "schema_version": 1,
            "commit": run("git", "rev-parse", "HEAD"),
            "dirty": bool(run("git", "status", "--porcelain")),
            "baseline_commit": "c6ca7fe8a7e6eb9bc7dad2904c92b3b824a36a82",
            "upstream_baseline": "0.153.4",
            "fork_release": "0.0.14",
            "supported_targets": TARGETS,
            "host": {"system": platform.system(), "machine": platform.machine()},
            "measurements": {
                "clean_build_seconds": None,
                "warm_build_seconds": None,
                "editable_prompt_seconds": None,
                "idle_rss_bytes": None,
            },
            "measurement_note": "null means unmeasured, never a passing gate; --version is not prompt startup",
        }
    )
    if args.binary:
        binary = args.binary.resolve(strict=True)
        digest = hashlib.sha256()
        with binary.open("rb") as stream:
            for block in iter(lambda: stream.read(1024 * 1024), b""):
                digest.update(block)
        start = time.monotonic()
        version = subprocess.check_output(
            [str(binary), "--version"], text=True, timeout=30
        ).strip()
        report["binary"] = {
            "path": str(binary),
            "bytes": binary.stat().st_size,
            "sha256": digest.hexdigest(),
            "version": version,
            "version_command_seconds": time.monotonic() - start,
            "stripped": "not verified",
        }
    print(json.dumps(report, indent=2, sort_keys=True))
    if args.check:
        return int(
            bool(report["forbidden_dependencies"])
            or len(report["production_workspace_crates"]) > 10
            or report["production_source_lines_upper_bound"] > 100000
            or bool(report["modules_over_800_lines"])
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
