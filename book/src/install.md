# Install

**Windows: the [Microsoft Store](https://apps.microsoft.com/detail/9N5CLXQLBQNF)** is the easiest:
signed, no "Windows protected your PC" prompt, and it updates itself. Store updates can come a
little after a GitHub release, and the Store version doesn't include `raptaboard-cli`; for those,
use the files below.

Or download the file for your computer from the
[latest release](https://github.com/tuffinmuffin/raptaboard/releases/latest):

| System | File |
| --- | --- |
| Windows, installed (Start menu, uninstaller; no admin needed) | `raptaboard-<version>-windows-x64-setup.exe` |
| Windows, portable (unzip and run; settings stay in its folder) | `raptaboard-<version>-windows-x64.zip` |
| macOS 11+, Apple Silicon and Intel | `raptaboard-<version>-macos-universal.zip` |
| Linux x64 (`./install.sh` adds it to your menu, for you only) | `raptaboard-<version>-linux-x64.tar.gz` |

These builds aren't signed yet, so the first start asks first:

- **Windows:** at "Windows protected your PC", click "More info", then "Run anyway".
- **macOS:** right-click raptaboard.app, choose Open, then Open again.

Each package has a README.txt with first steps. To build raptaboard yourself, see
[Building from source](dev/building.md).
