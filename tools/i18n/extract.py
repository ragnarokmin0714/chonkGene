#!/usr/bin/env python3
"""Gather the words the program shows into the translation files.

    python3 tools/i18n/extract.py

Finds every `tr("…")`, `trf("…", …)` and `tr_in(…, "…")` in the Rust code and
every `@tr("…")` in the .slint files, and merges them into
i18n/<lang>/LC_MESSAGES/chonkgene.po for each language there: a string already
translated keeps its translation, a new one comes in empty (it shows in
Chinese until it is translated), one no longer in the code is dropped. Each
entry says where it is used (`#:`). The order is the order they are found in.

Only the standard library is needed; nothing here runs in the app.
"""

import glob
import os
import re

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.normpath(os.path.join(HERE, "..", ".."))
STRING = r'"((?:[^"\\]|\\.)*)"'
RUST = re.compile(r'(?<![\w@])(?:tr|trf)\(\s*' + STRING + r'|tr_in\([^,]+,\s*' + STRING)
SLINT = re.compile(r'@tr\(\s*' + STRING)


def found():
    """msgid -> list of "file:line", in the order first found."""
    out = {}
    files = sorted(glob.glob(os.path.join(ROOT, "crates", "**", "*.rs"), recursive=True))
    files += sorted(glob.glob(os.path.join(ROOT, "crates", "**", "*.slint"), recursive=True))
    for path in files:
        rel = os.path.relpath(path, ROOT)
        # i18n.rs itself: its tests' samples.
        if "/tests/" in rel or "/examples/" in rel or rel.endswith("i18n.rs"):
            continue
        pattern = SLINT if path.endswith(".slint") else RUST
        with open(path, encoding="utf-8") as f:
            for n, line in enumerate(f, 1):
                if line.lstrip().startswith("//"):
                    continue
                for m in pattern.finditer(line):
                    msgid = next(g for g in m.groups() if g is not None)
                    if msgid:
                        out.setdefault(msgid, []).append(f"{rel}:{n}")
    return out


def unquote(s):
    s = s.strip()
    if s.startswith('"') and s.endswith('"'):
        s = s[1:-1]
    return s


def read_po(path):
    """(header lines, {msgid: msgstr}) with strings kept escaped as in the file."""
    header, entries = [], {}
    if not os.path.exists(path):
        return header, entries
    msgid = msgstr = None
    into = None
    in_header = True
    with open(path, encoding="utf-8") as f:
        for raw in f:
            line = raw.rstrip("\n")
            t = line.strip()
            if in_header:
                header.append(line)
                if t == "" and len(header) > 1:
                    in_header = False
                continue
            if t.startswith("msgid "):
                if msgid is not None:
                    entries[msgid] = msgstr or ""
                msgid, msgstr, into = unquote(t[6:]), None, "id"
            elif t.startswith("msgstr "):
                msgstr, into = unquote(t[7:]), "str"
            elif t.startswith('"'):
                if into == "id":
                    msgid += unquote(t)
                elif into == "str":
                    msgstr += unquote(t)
    if msgid is not None:
        entries[msgid] = msgstr or ""
    while header and header[-1].strip() == "":
        header.pop()
    return header, entries


def write_po(path, header, strings, old):
    with open(path, "w", encoding="utf-8") as f:
        f.write("\n".join(header) + "\n")
        for msgid, where in strings.items():
            f.write("\n")
            for w in where[:3]:
                f.write(f"#: {w}\n")
            f.write(f'msgid "{msgid}"\n')
            f.write(f'msgstr "{old.get(msgid, "")}"\n')


def main():
    strings = found()
    for po in sorted(glob.glob(os.path.join(ROOT, "i18n", "*", "LC_MESSAGES", "chonkgene.po"))):
        header, old = read_po(po)
        write_po(po, header, strings, old)
        done = sum(1 for k in strings if old.get(k))
        dropped = [k for k in old if k not in strings]
        lang = po.split(os.sep)[-3]
        print(f"{lang}: {len(strings)} strings, {done} translated, {len(dropped)} dropped")


if __name__ == "__main__":
    main()
