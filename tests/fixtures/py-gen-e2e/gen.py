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

# Find the --seed argument, which is passed as --seed=...
seed = "default_seed"
for arg in sys.argv:
    if arg.startswith("--seed="):
        seed = arg.split("=")[1]

# Print one normal test case: id "test_case_1"
print(f"test_case_1,{seed},10,20,30")

# Print one embedded json metadata test case:
# eyJ0ZXN0X21ldGEiOiAidmFsdWUiLCJzZWVkIjogNDJ9 -> {"test_meta": "value","seed": 42}
print(f"data:application/json;base64,eyJ0ZXN0X21ldGEiOiAidmFsdWUiLCJzZWVkIjogNDJ9,{seed},10,20,30")
