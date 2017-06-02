#!/usr/bin/env python3

import sys
import subprocess as sp

program = ["target/release/ewok"]
timeout = 180

def main():
    i = 0
    while True:
        print("run {}".format(i))
        try:
            sp.run(program, check=True, timeout=timeout)
        except sp.CalledProcessError as e:
            sys.exit(1)
        except sp.TimeoutExpired as e:
            print("timed out after {} seconds".format(e.timeout))
        i += 1

if __name__ == "__main__":
    main()
