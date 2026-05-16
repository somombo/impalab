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
import argparse

def run(target: str, data = [], **kwargs):
    if target == 'test_func_1':
        print("Info: Running `test_func_1` ...", file=sys.stderr)
        return 1234
        
    elif target == 'test_func_2':
        print("Info: Running `test_func_2` ...", file=sys.stderr)
        return len("".join(kwargs.keys())) + len(data) + 1
    else:
        print(f"Error: Unsupported target: '{target}'", file=sys.stderr)
        sys.exit(1)
        return

def main():
    parser = argparse.ArgumentParser(prog='python-e2e')
    parser.add_argument('subcommand', help='The target to execute')
    args, unknown_args = parser.parse_known_args()
    kwargs = {}
    for arg in unknown_args:
        if arg.startswith('--') and '=' in arg:
            key, val = arg[2:].split('=', 1)
            kwargs[key] = val
        else:
            print(f"Error: Invalid flag format '{arg}'. Expected --key=value")
            sys.exit(1)
            return

    target = args.subcommand
    print(f'Info: Executed target `{target}` with kwargs {kwargs}', file=sys.stderr)

    for line in sys.stdin:
        line = line.strip()
        if not line:
            print(f'Error: Line is empty before EOF', file=sys.stderr)
            sys.exit(1)
        
        # We need to split by comma, but some comma might be part of the test data ID
        # Wait, the test input `data:application/json;base64,eyJ0ZXN0X21ldGEiOiAidmFsdWUiLCJzZWVkIjogNDJ9`
        # actually has a comma in it!
        # So parts[0] is `data:application/json;base64`, not the full thing!

        # We should find the first comma if it's a regular id,
        # but what if it's the `data:application/json;base64,` prefix?
        # Actually in gen.py, the id is outputted, and then followed by other elements
        # separated by commas:
        # print(f"data:application/json;base64,eyJ0ZXN0X21ldGEiOiAidmFsdWUiLCJzZWVkIjogNDJ9,{seed},10,20,30")

        # So let's parse differently based on prefix
        if line.startswith("data:application/json;base64,"):
            # find the second comma!
            # first comma is at len("data:application/json;base64")
            first_comma = len("data:application/json;base64")
            second_comma = line.find(",", first_comma + 1)

            if second_comma == -1:
                print(f'Error: Line is malformed. Cannot parse: `{line}`', file=sys.stderr)
                sys.exit(1)

            test_data_id = line[:second_comma]
            input_data = line[second_comma+1:].split(",")
        else:
            parts = line.split(",")
            if len(parts) < 2:
                print(f'Error: Line is malformed. Cannot parse: `{line}`', file=sys.stderr)
                sys.exit(1)

            test_data_id = parts[0]
            input_data = parts[1:]

        print(f'Info: Received generated data: `{(test_data_id, input_data)}`', file=sys.stderr)
        
        example_duration = run(target, input_data, **kwargs)

        print(f"{test_data_id},{example_duration}")

if __name__ == '__main__':
    main()
