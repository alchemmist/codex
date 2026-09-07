import argparse
import os
from pathlib import Path
import shlex
import subprocess


ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--host", default=os.environ.get("ANTEX_BUILD_HOST"))
    parser.add_argument("--checkout", required=True)
    parser.add_argument("--tools", required=True)
    parser.add_argument("--jobs", type=int, default=16)
    parser.add_argument("--tty", action="store_true")
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    if not args.host or not args.command or args.jobs < 1:
        parser.error("a build host, command, and positive job count are required")
    workspace = "antex-rs"
    command = args.command[1:] if args.command[0] == "--" else args.command
    if not command:
        parser.error("a command is required")
    environment = [
        f"PATH={args.tools}/bin:{args.tools}/git/bin:{args.tools}/cargo/bin:/usr/local/bin:/usr/bin:/bin",
        f"CARGO_HOME={args.tools}/cargo",
        f"RUSTUP_HOME={args.tools}/rustup",
        f"CARGO_BUILD_JOBS={args.jobs}",
        "CARGO_PROFILE_DEV_DEBUG=0",
        "CARGO_PROFILE_TEST_DEBUG=0",
        "CARGO_INCREMENTAL=0",
        "CC=clang-17",
        "CXX=clang++-17",
        "TMPDIR=/tmp",
        "TERM=xterm-256color",
        "TERM_PROGRAM=Apple_Terminal",
        "COLORTERM=truecolor",
        "NO_COLOR=1",
    ]
    remote = f"umask 022 && cd {shlex.quote(args.checkout + '/' + workspace)} && "
    remote += shlex.join(["env", *environment, *command])
    return subprocess.run(
        [
            "ssh",
            *(["-tt"] if args.tty else []),
            "-o",
            "BatchMode=yes",
            "--",
            args.host,
            remote,
        ]
    ).returncode


if __name__ == "__main__":
    raise SystemExit(main())
