#!/usr/bin/env python3
"""Keep the measured shadow optimization enabled in the shipping renderer."""
import sys
import tomllib

with open(sys.argv[1], "rb") as manifest:
    features = tomllib.load(manifest)["dependencies"]["vello_cpu"]["features"]
if "poly-pnorm" not in features:
    sys.exit("hotty-blitz must enable vello_cpu/poly-pnorm (R-33)")
