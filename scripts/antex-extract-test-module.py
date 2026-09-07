import argparse
from pathlib import Path
import textwrap

from tree_sitter import Language, Parser
import tree_sitter_rust


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("source", type=Path)
    parser.add_argument("destination", type=Path)
    parser.add_argument("--module", required=True)
    parser.add_argument("--keep-module", action="store_true")
    args = parser.parse_args()
    source = args.source.read_bytes()
    tree = Parser(Language(tree_sitter_rust.language())).parse(source)
    if tree.root_node.has_error:
        raise SystemExit("Rust source does not parse")
    nodes = list(tree.root_node.children)
    target = next(
        node
        for node in nodes
        if node.type == "mod_item"
        and source[
            node.child_by_field_name("name").start_byte : node.child_by_field_name(
                "name"
            ).end_byte
        ]
        == b"tests"
    )
    body = target.child_by_field_name("body")
    start = target.start_byte
    index = nodes.index(target) - 1
    while index >= 0 and nodes[index].type == "attribute_item":
        start = nodes[index].start_byte
        index -= 1
    extracted = textwrap.dedent(
        source[body.start_byte + 1 : body.end_byte - 1].decode()
    ).lstrip()
    if not args.keep_module:
        extracted = extracted.replace(
            "use super::*;", f"use crate::{args.module}::*;", 1
        )
    if args.destination.exists():
        raise SystemExit("destination already exists")
    args.destination.write_text(extracted)
    replacement = b""
    if args.keep_module:
        replacement = (
            f'#[cfg(test)]\n#[path = "{args.destination.name}"]\nmod tests;'.encode()
        )
    args.source.write_bytes(source[:start] + replacement + source[target.end_byte :])


if __name__ == "__main__":
    main()
