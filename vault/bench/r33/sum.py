#!/usr/bin/env python3
"""R-33 leg 2: sum up run.sh's samples.

    sum.py OUT [BASE PATCHED [AA]]

With no labels, one table per build: per document, the paint stage's
median over every frame, its p10 and p90, and the range of the rounds'
medians. With BASE and PATCHED, a comparison: each round's median paired
with the other build's in the same round, the change in the median of all
frames, and the range of the paired rounds' changes. A change is called a
win or a loss only when every round moves the same way.
"""
import os
import statistics
import sys


def load(path):
    rounds = {}
    for line in open(path):
        r, _i, _res, paint, _wall, _area = line.split("\t")
        rounds.setdefault(int(r), []).append(int(paint) / 1000)
    return rounds


def q(v, p):
    v = sorted(v)
    return v[round((len(v) - 1) * p)]


def docs(out, label):
    d = os.path.join(out, label)
    return sorted(f[:-4] for f in os.listdir(d) if f.endswith(".tsv") and not f.startswith("."))


def table(out, label):
    print(f"## {label}\n")
    print("| document | frames | paint p50 ms | p10 | p90 | rounds' p50 |")
    print("| --- | ---: | ---: | ---: | ---: | --- |")
    for doc in docs(out, label):
        rounds = load(os.path.join(out, label, doc + ".tsv"))
        allv = [x for v in rounds.values() for x in v]
        meds = [statistics.median(v) for v in rounds.values()]
        print(f"| {doc} | {len(allv)} | {statistics.median(allv):.2f} | {q(allv, .1):.2f} | {q(allv, .9):.2f} | {min(meds):.2f}–{max(meds):.2f} |")
    print()


def paired(out, a, b, doc):
    ra, rb = load(os.path.join(out, a, doc + ".tsv")), load(os.path.join(out, b, doc + ".tsv"))
    return [
        (statistics.median(rb[r]) - statistics.median(ra[r])) / statistics.median(ra[r]) * 100
        for r in sorted(ra) if r in rb and statistics.median(ra[r]) > 0
    ]


def compare(out, a, b, aa=None):
    print(f"## {b} against {a}\n")
    print(f"| document | {a} p50 ms | {b} p50 ms | change | paired median | rounds' change | faster | call |")
    print("| --- | ---: | ---: | ---: | ---: | --- | ---: | --- |")
    for doc in docs(out, a):
        pb = os.path.join(out, b, doc + ".tsv")
        if not os.path.exists(pb):
            continue
        ra, rb = load(os.path.join(out, a, doc + ".tsv")), load(pb)
        ma = statistics.median([x for v in ra.values() for x in v])
        mb = statistics.median([x for v in rb.values() for x in v])
        diffs = [
            (statistics.median(rb[r]) - statistics.median(ra[r])) / statistics.median(ra[r]) * 100
            for r in sorted(ra) if r in rb and statistics.median(ra[r]) > 0
        ]
        n, k = len(diffs), sum(d < 0 for d in diffs)
        pm = statistics.median(diffs) if diffs else 0
        call = "noise"
        bar = 5.0
        if aa and os.path.exists(os.path.join(out, aa, doc + ".tsv")):
            bar = max(bar, 2 * abs(statistics.median(paired(out, a, aa, doc))))
        if n and k >= n - 1 and pm < -bar:
            call = "win"
        elif n and n - k >= n - 1 and pm > bar:
            call = "loss"
        ch = (mb - ma) / ma * 100 if ma else 0
        rng = f"{min(diffs):+.1f}% to {max(diffs):+.1f}%" if diffs else "–"
        print(f"| {doc} | {ma:.2f} | {mb:.2f} | {ch:+.1f}% | {pm:+.1f}% | {rng} | {k}/{n} | {call} (bar {bar:.0f}%) |")
    print()


if __name__ == "__main__":
    out = sys.argv[1]
    if len(sys.argv) in (4, 5):
        compare(out, *sys.argv[2:])
    else:
        for label in sorted(os.listdir(out)):
            if os.path.isdir(os.path.join(out, label)):
                table(out, label)
