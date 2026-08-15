import hashlib
import urllib.request
from pathlib import Path

RELEASE_TAG = "20230704"
NAME = "hefty-water.epub"
URL = f"https://github.com/IDPF/epub3-samples/releases/download/{RELEASE_TAG}/{NAME}"
SHA256 = "1133a872bbe50df15b9a4928c33f693fd264f83276041b478aa6b076b1183fb3"

CACHE_DIR = Path(__file__).resolve().parent.parent / ".cache"


def fetch() -> Path:
    path = CACHE_DIR / f"{RELEASE_TAG}-{NAME}"
    if not _is_valid(path):
        CACHE_DIR.mkdir(parents=True, exist_ok=True)
        with urllib.request.urlopen(URL, timeout=60) as response:
            path.write_bytes(response.read())
        if not _is_valid(path):
            raise AssertionError(f"checksum mismatch for {URL}")
    return path


def _is_valid(path: Path) -> bool:
    if not path.is_file():
        return False
    return hashlib.sha256(path.read_bytes()).hexdigest() == SHA256
