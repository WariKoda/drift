# Third-party notices for diff syntax highlighting

Drift's optional Go syntax highlighting uses the following dependencies.
Include these notices when distributing binaries containing them. This file
covers the syntax-highlighting dependencies, not every dependency in Drift.

- [Chroma v2.27.0](https://github.com/alecthomas/chroma), MIT license.
  Copyright (C) 2017 Alec Thomas.
- [regexp2/v2 v2.2.1](https://github.com/dlclark/regexp2), MIT license.
  Copyright (c) Doug Clark.

Both projects grant permission under the following terms:

> Permission is hereby granted, free of charge, to any person obtaining a copy of
> this software and associated documentation files (the "Software"), to deal in
> the Software without restriction, including without limitation the rights to
> use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of
> the Software, and to permit persons to whom the Software is furnished to do so,
> subject to the following conditions:
>
> The above copyright notice and this permission notice shall be included in all
> copies or substantial portions of the Software.
>
> THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
> IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
> FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
> AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
> LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
> OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
> SOFTWARE.

Drift imports Chroma's tokenization and lexer packages, not its SVG formatter
or bundled font. No Hunk source code is included.
