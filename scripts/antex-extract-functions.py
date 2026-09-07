import argparse
from pathlib import Path

from tree_sitter import Language, Parser
import tree_sitter_rust


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("source", type=Path)
    parser.add_argument("destination", type=Path)
    parser.add_argument("--first", required=True)
    parser.add_argument("--last", required=True)
    args = parser.parse_args()
    source = args.source.read_bytes()
    tree = Parser(Language(tree_sitter_rust.language())).parse(source)
    if tree.root_node.has_error or args.destination.exists():
        raise SystemExit("invalid source or existing destination")
    functions = [
        node for node in tree.root_node.named_children if node.type == "function_item"
    ]
    names = [
        source[
            node.child_by_field_name("name").start_byte : node.child_by_field_name(
                "name"
            ).end_byte
        ].decode()
        for node in functions
    ]
    selected = functions[names.index(args.first) : names.index(args.last) + 1]
    if not selected:
        raise SystemExit("empty range")
    start = selected[0].start_byte
    previous = selected[0].prev_named_sibling
    while previous and previous.type in {
        "line_comment",
        "block_comment",
        "attribute_item",
    }:
        start = previous.start_byte
        previous = previous.prev_named_sibling
    end = selected[-1].end_byte
    extracted = source[start:end]
    for node in reversed(selected):
        if not any(child.type == "visibility_modifier" for child in node.children):
            offset = node.start_byte - start
            extracted = extracted[:offset] + b"pub(super) " + extracted[offset:]
    args.destination.write_bytes(b"use super::*;\n\n" + extracted + b"\n")
    args.source.write_bytes(source[:start] + source[end:])


if __name__ == "__main__":
    main()
