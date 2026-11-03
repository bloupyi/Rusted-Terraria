#!/usr/bin/env python3
"""Rejects game binaries, assets and decompiled sources, even after git add -f."""

import subprocess
import sys
from pathlib import PurePosixPath

PRIVATE_ROOTS = {".local", "assets", "Content", "decompiled"}
PRIVATE_EXTENSIONS = {
    ".cs", ".csproj", ".sln", ".exe", ".dll", ".pdb",
    ".xnb", ".wld", ".twld", ".plr", ".xwb", ".xsb", ".xgs",
}


def prohibited(path):
    p = PurePosixPath(path)
    return bool(p.parts) and (p.parts[0] in PRIVATE_ROOTS or p.suffix.lower() in PRIVATE_EXTENSIONS)


def main():
    if "--history" in sys.argv:
        out = subprocess.check_output(["git", "log", "--all", "-m", "--format=", "--name-only", "-z"])
    else:
        out = subprocess.check_output(["git", "ls-files", "-z"])
    bad = sorted({p.strip("\n") for p in out.decode().split("\0") if p.strip("\n") and prohibited(p.strip("\n"))})
    if bad:
        print("Fichiers du jeu ou sources decompilees dans le depot :", file=sys.stderr)
        for p in bad:
            print("  " + p, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
