#! /usr/bin/env python3

import subprocess
import re

output = subprocess.run(
        ["cargo", "+nightly", "nm", "--features"," nightly", "--", "-S", "--size-sort"],
        text=True, capture_output=True
).stdout

total_size = 0
for line in output.splitlines():
    match = re.search(r"^[0-9a-fA-F]+ ([0-9a-fA-F]+)\s+.*:(\S+)::POOL::", line)
    if match:
        size = int(match.group(1), 16)
        name = match.group(2)
        print(f"{size:5d} {name}")
        total_size += size

print("-----")
print(f"{total_size:5d} Total bytes")
