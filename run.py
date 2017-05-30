#!/usr/bin/env python3

import sys
import subprocess as sp

program = ["target/release/ewok"]

def main():
    i = 0
    while True:
        print("run {}".format(i))
        try:
            sp.run(program, stdout=sp.PIPE, check=True)
        except sp.CalledProcessError as e:
            print(e.output)
            sys.exit(1)
        i += 1

if __name__ == "__main__":
    main()
