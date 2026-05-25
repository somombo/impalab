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
import os
import sys

seed_str = os.environ.get("IMPALAB_SEED")
if seed_str is None:
    print("Error: IMPALAB_SEED environment variable is not set", file=sys.stderr)
    sys.exit(1)

try:
    seed = int(seed_str)
    if seed < 0 or seed > 18446744073709551615:
        raise ValueError()
except ValueError:
    print(f"Error: IMPALAB_SEED '{seed_str}' is not a valid u64", file=sys.stderr)
    sys.exit(1)

# Print one test case: id "test_case_1"
# We'll include the seed in the output to prove it was received.
print(f"test_case_1,{seed},10,20,30")
