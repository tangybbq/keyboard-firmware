#! /usr/bin/env python3

import subprocess
import re

output = subprocess.run(
        ["cargo", "+nightly", "nm", "--features"," nightly", "--", "-S"],
        text=True, capture_output=True
).stdout

total_size = 0
for line in output.splitlines():
    match = re.search(r"^[0-9a-fA-F]+ ([0-9a-fA-F]+)\s+.*::POOL::", line)
    if match:
        size = int(match.group(1), 16)
        total_size += size

print(f"Total POOL size: {total_size} bytes")
