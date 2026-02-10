"""Bivvy CLI - Cross-language development environment setup automation."""

import platform
import subprocess
import sys
import tarfile
import urllib.request
from pathlib import Path

__version__ = "1.0.1"

BINARY_DIR = Path(__file__).parent / "bin"
BINARY_NAME = "bivvy.exe" if sys.platform == "win32" else "bivvy"
BINARY_PATH = BINARY_DIR / BINARY_NAME
GITHUB_REPO = "bivvy-dev/bivvy"


def get_platform() -> str:
    """Get the platform string for downloads."""
    system = platform.system().lower()
    machine = platform.machine().lower()

    os_map = {"darwin": "darwin", "linux": "linux", "windows": "windows"}
    arch_map = {"x86_64": "x64", "amd64": "x64", "arm64": "arm64", "aarch64": "arm64"}

    os_name = os_map.get(system)
    arch_name = arch_map.get(machine)

    if not os_name or not arch_name:
        raise RuntimeError(f"Unsupported platform: {system}-{machine}")

    return f"{os_name}-{arch_name}"


def download_binary() -> None:
    """Download the bivvy binary for the current platform."""
    plat = get_platform()
    url = f"https://github.com/{GITHUB_REPO}/releases/download/{__version__}/bivvy-{plat}.tar.gz"

    print(f"Downloading bivvy from {url}")

    BINARY_DIR.mkdir(parents=True, exist_ok=True)

    with urllib.request.urlopen(url) as response:  # noqa: S310
        with tarfile.open(fileobj=response, mode="r:gz") as tar:
            for member in tar.getmembers():
                if member.name == "bivvy":
                    member.name = BINARY_NAME
                    tar.extract(member, BINARY_DIR)

    if sys.platform != "win32":
        BINARY_PATH.chmod(0o755)

    print("bivvy installed successfully")


def ensure_binary() -> Path:
    """Ensure the binary exists, downloading if necessary."""
    if not BINARY_PATH.exists():
        download_binary()
    return BINARY_PATH


def main() -> None:
    """Run the bivvy binary."""
    binary = ensure_binary()
    result = subprocess.run([str(binary), *sys.argv[1:]], check=False)
    sys.exit(result.returncode)


if __name__ == "__main__":
    main()
