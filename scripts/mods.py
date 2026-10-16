#!/usr/bin/env python3
"""The mods (gov P-7): the patches this repository carries on upstream
software, one set per upstream in mods/<upstream>/, each with mod.md and a
<name>.md per patch. The rules match web's shared/mods, which neuroplast.io's
Modding page reads them with.

    mods.py check [ROOT]        every set under ROOT/mods (default: here)
    mods.py base SET            a set's base, from its mod.md
    mods.py relink SET NEWDIR   point each .md at the patch in NEWDIR with
                                its patch's subject, after an export

`check` is P-7 rule 5 but for whether the patches apply on base, which the
build tries (make apply). It names every problem, a line each, and fails if
there is one.
"""
import email
import email.policy
import os
import re
import sys

FITS = ("yes", "maybe", "no")
MAX_BODY = 2048
MOD_FIELDS = ("upstream", "url", "license", "base", "builds", "public")
PATCH_FIELDS = ("title", "patch", "upstream-fit")
PATCH_FILE = re.compile(r"^[0-9]{4}-[^/]+\.patch$")
MD_NAME = re.compile(r"^[a-z0-9]+(-[a-z0-9]+)*\.md$")
# The site shows a body inside its own page: no headings, no rules.
HEADING = re.compile(r"^ {0,3}(#{1,6}(\s|$)|(-{3,}|\*{3,}|_{3,}|={3,})\s*$)")


def front_matter(text):
    """Front matter fields and the body, in the part of YAML P-7 uses: one
    `key: value` a line, plain or quoted, and # comments."""
    text = text.replace("\r\n", "\n")
    if not text.startswith("---\n"):
        raise ValueError("no front matter: the file starts with a line of ---")
    rest = text[4:]
    head, sep, body = rest.partition("\n---\n")
    if not sep:
        if not rest.endswith("\n---"):
            raise ValueError("the front matter has no closing line of ---")
        head, body = rest[:-4], ""
    fields = {}
    for i, line in enumerate(head.split("\n")):
        t = line.strip()
        if not t or t.startswith("#"):
            continue
        k, colon, v = line.partition(":")
        if not colon or not k or k != k.strip() or any(c in k for c in " \t\"'"):
            raise ValueError(f"front matter line {i + 2} is not `key: value`")
        if k in fields:
            raise ValueError(f"front matter line {i + 2}: {k} again")
        try:
            fields[k] = scalar(v)
        except ValueError as e:
            raise ValueError(f"front matter line {i + 2}: {k}: {e}")
    return fields, body.strip()


def scalar(v):
    v = v.strip()
    if not v:
        return ""
    if v[0] in "\"'":
        q, out, i = v[0], [], 1
        while i < len(v):
            c = v[i]
            if c == q == "'" and v[i + 1 : i + 2] == "'":
                out.append("'")
                i += 2
                continue
            if c == q:
                after = v[i + 1 :].strip()
                if after and not after.startswith("#"):
                    raise ValueError(f"{after!r} after the closing quote")
                return "".join(out)
            if c == "\\" and q == '"' and i + 1 < len(v):
                i += 1
                esc = {'"': '"', "\\": "\\", "/": "/", "n": "\n", "t": "\t"}
                if v[i] not in esc:
                    raise ValueError(f"the escape \\{v[i]}, which this reader doesn't take")
                out.append(esc[v[i]])
            else:
                out.append(c)
            i += 1
        raise ValueError("no closing quote")
    if v[0] in "[]{}&*!|>%@`":
        raise ValueError(f"{v!r}: a plain value, or one in quotes")
    for mark in (" #", "\t#"):
        if mark in v:
            v = v[: v.index(mark)]
    v = v.strip()
    if ": " in v:
        raise ValueError(f'{v!r}: a value with ": " goes in quotes')
    return v


def read_set(d):
    """The problems with the set in directory d, as `file: problem` lines."""
    errs = []

    def fail(name, msg):
        errs.append(f"{os.path.join(d, name)}: {msg}")

    patches, mds, has_mod = [], [], False
    for name in sorted(os.listdir(d)):
        if os.path.isdir(os.path.join(d, name)):
            fail(name, "a directory, where a set holds only mod.md, patches and their .md")
        elif name == "mod.md":
            has_mod = True
        elif name.endswith(".patch"):
            if not PATCH_FILE.match(name):
                fail(name, "a patch is NNNN-<subject>.patch, as git format-patch names it")
            patches.append(name)
        elif name.endswith(".md"):
            if not MD_NAME.match(name):
                fail(name, "a patch's .md is named in short kebab-case")
            mds.append(name)
        else:
            fail(name, "not mod.md, a patch or a patch's .md")
    if not has_mod:
        fail("mod.md", "missing: it says what the set is")
    else:
        try:
            fields, _ = front_matter(open(os.path.join(d, "mod.md")).read())
        except ValueError as e:
            fail("mod.md", str(e))
        else:
            for k in MOD_FIELDS:
                if not fields.get(k):
                    fail("mod.md", f"no {k}")
            url = fields.get("url", "")
            if url and (not url.startswith("https://") or any(c in url for c in " \t<>\"'`")):
                fail("mod.md", f"url is {url!r}, not an https:// address")
            if fields.get("public", "") not in ("true", "false", ""):
                fail("mod.md", f"public is {fields['public']!r}: true or false")
    named = {}
    for md in mds:
        try:
            fields, body = front_matter(open(os.path.join(d, md)).read())
        except ValueError as e:
            fail(md, str(e))
            continue
        for k in PATCH_FIELDS:
            if not fields.get(k):
                fail(md, f"no {k}")
        fit = fields.get("upstream-fit", "")
        if fit and fit not in FITS:
            fail(md, f"upstream-fit is {fit!r}: {', '.join(FITS)}")
        if len(body.encode()) > MAX_BODY:
            fail(md, f"the body is {len(body.encode())} bytes, over {MAX_BODY}")
        for n, line in enumerate(body.split("\n")):
            if HEADING.match(line):
                fail(md, f"body line {n + 1} is a heading or a rule, which the site's page has its own of")
        p = fields.get("patch", "")
        if p:
            if p not in patches:
                fail(md, f"patch {p} is not in the set")
            elif p in named:
                fail(md, f"names {p}, which {named[p]} names too")
            else:
                named[p] = md
    for p in patches:
        if p not in named:
            fail(p, "no .md names it")
    if not patches:
        fail("", "no patches: a set without any is deleted (P-7 rule 8)")
    return errs


def check(root):
    base = os.path.join(root, "mods")
    sets = sorted(os.path.join(base, n) for n in os.listdir(base)) if os.path.isdir(base) else []
    sets = [s for s in sets if os.path.isdir(s)]
    errs, n = [], 0
    for s in sets:
        errs += read_set(s)
        n += sum(1 for f in os.listdir(s) if f.endswith(".patch"))
    for e in errs:
        print("mods:", e, file=sys.stderr)
    if sets:
        print(f"mods: {len(sets)} set(s), {n} patch(es)")
    return 1 if errs else 0


def subject(path):
    with open(path, "rb") as f:
        msg = email.message_from_binary_file(f, policy=email.policy.default)
    return re.sub(r"^\[PATCH[^\]]*\]\s*", "", " ".join(str(msg["Subject"] or "").split()))


def relink(d, new):
    """Each .md's patch: line, from the patch in d to the one in new with the
    same subject. A subject that changed is left for the author, and check
    names it."""
    by_subject = {subject(os.path.join(new, f)): f for f in os.listdir(new) if f.endswith(".patch")}
    left = 0
    for md in sorted(f for f in os.listdir(d) if f.endswith(".md") and f != "mod.md"):
        path = os.path.join(d, md)
        text = open(path).read()
        fields, _ = front_matter(text)
        old = os.path.join(d, fields.get("patch", ""))
        to = by_subject.get(subject(old)) if os.path.isfile(old) else None
        if to is None:
            print(f"mods: {path}: no exported patch has its patch's subject; set patch: by hand", file=sys.stderr)
            left += 1
        elif to != fields["patch"]:
            text = re.sub(r"(?m)^patch:.*$", f"patch: {to}", text, count=1)
            open(path, "w").write(text)
    return 1 if left else 0


def main(argv):
    if len(argv) >= 1 and argv[0] == "check" and len(argv) <= 2:
        return check(argv[1] if len(argv) == 2 else ".")
    if len(argv) == 2 and argv[0] == "base":
        print(front_matter(open(os.path.join(argv[1], "mod.md")).read())[0]["base"])
        return 0
    if len(argv) == 3 and argv[0] == "relink":
        return relink(argv[1], argv[2])
    print(__doc__.strip(), file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
