#!/usr/bin/env python3
"""Write translations into a language's .po file from a table.

    python3 tools/i18n/fill.py en tools/i18n/en.tsv

The table has one string a line: the Chinese, " → ", the
translation (`#` lines and blank lines are skipped). Each entry of
i18n/<lang>/LC_MESSAGES/chonkgene.po whose msgid is in the table gets that
msgstr; entries not in it are left as they are. Prints what is still
untranslated. Run tools/i18n/extract.py first, so every string is there.
"""

import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.normpath(os.path.join(HERE, "..", ".."))


def esc(s):
    return s.replace("\\", "\\\\").replace('"', '\\"')


def main():
    lang, table = sys.argv[1], sys.argv[2]
    words = {}
    with open(table, encoding="utf-8") as f:
        for line in f:
            line = line.rstrip("\n")
            if not line or line.startswith("#"):
                continue
            zh, _, out = line.partition(" → ")
            words[zh] = out
    path = os.path.join(ROOT, "i18n", lang, "LC_MESSAGES", "chonkgene.po")
    with open(path, encoding="utf-8") as f:
        lines = f.read().split("\n")
    missing = []
    for i, line in enumerate(lines):
        if line.startswith('msgid "') and line != 'msgid ""':
            zh = line[7:-1].replace('\\"', '"')
            if i + 1 < len(lines) and lines[i + 1].startswith("msgstr "):
                if zh in words and words[zh]:
                    lines[i + 1] = f'msgstr "{esc(words[zh])}"'
                elif lines[i + 1] == 'msgstr ""':
                    missing.append(zh)
    with open(path, "w", encoding="utf-8") as f:
        f.write("\n".join(lines))
    print(f"{lang}: {len(missing)} untranslated")
    for zh in missing:
        print("  " + zh)


if __name__ == "__main__":
    main()
