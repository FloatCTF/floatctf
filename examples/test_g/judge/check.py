#!/usr/bin/env python3
"""GameBox functional check (judge).

Trusted by the platform; never baked into the Docker image.
Exit 0 = service OK, non-zero = down / broken.
"""
import sys


def main() -> int:
    # TODO: probe the target (env/args provided by judgeserver)
    print("ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
