# Copyright 2025 Chisomo Makombo Sakala
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.
import sys
import time

functions = []
for arg in sys.argv:
    if arg.startswith("--functions="):
        functions = arg.split("=")[1].split(",")

# Read every line from stdin
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    
    parts = line.split(",")
    test_id = parts[0]
    
    # For each function, print a result.
    for func in functions:
        # Fake a duration (1234ns) and print the CSV output
        print(f"{test_id},{func},1234")
