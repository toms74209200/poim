import struct
import subprocess
import tempfile
from dataclasses import dataclass, field
from pathlib import Path

from wasmtime import Instance, Module, Store

STATUS_OK = 0
FORMAT_EPUB = 0
FORMAT_PDF = 1
SUFFIXES = {FORMAT_EPUB: "epub", FORMAT_PDF: "pdf"}


@dataclass
class Conversion:
    markdown: str = ""
    images: dict[str, bytes] = field(default_factory=dict)
    error: str | None = None


class _Cursor:
    def __init__(self, data: bytes):
        self.data = data
        self.pos = 0

    def u32(self) -> int:
        (value,) = struct.unpack_from("<I", self.data, self.pos)
        self.pos += 4
        return value

    def blob(self) -> bytes:
        size = self.u32()
        value = self.data[self.pos : self.pos + size]
        self.pos += size
        return value

    def text(self) -> str:
        return self.blob().decode()


def unpack(packed: bytes) -> Conversion:
    cursor = _Cursor(packed)
    total = cursor.u32()
    assert total == len(packed), f"declared length {total} != actual {len(packed)}"
    if cursor.u32() != STATUS_OK:
        return Conversion(error=cursor.text())

    markdown = cursor.text()
    images = {}
    for _ in range(cursor.u32()):
        path = cursor.text()
        images[path] = cursor.blob()
    assert cursor.pos == len(packed), "trailing bytes in packed result"
    return Conversion(markdown=markdown, images=images)


class WasmFrontend:
    BUILD_PATH = Path("target/wasm32-unknown-unknown/release/poim.wasm")

    def __init__(self, repo_root: Path):
        path = repo_root / self.BUILD_PATH
        assert path.is_file(), f"not built: {path}"
        self.store = Store()
        module = Module.from_file(self.store.engine, str(path))
        self.exports = Instance(self.store, module, []).exports(self.store)

    def convert(self, document: bytes, format: int = FORMAT_EPUB) -> Conversion:
        pointer = self.exports["alloc"](self.store, len(document))
        assert pointer != 0, "alloc returned a null pointer"
        memory = self.exports["memory"]
        memory.write(self.store, document, pointer)

        result = self.exports["convert"](self.store, pointer, len(document), format)
        self.exports["free"](self.store, pointer, len(document))

        total = struct.unpack_from("<I", memory.read(self.store, result, result + 4))[0]
        packed = bytes(memory.read(self.store, result, result + total))
        self.exports["free"](self.store, result, total)
        return unpack(packed)


class CliFrontend:
    BUILD_PATH = Path("target/release/poim")

    def __init__(self, repo_root: Path):
        self.binary = repo_root / self.BUILD_PATH
        assert self.binary.is_file(), f"not built: {self.binary}"

    def convert(self, document: bytes, format: int = FORMAT_EPUB) -> Conversion:
        with tempfile.TemporaryDirectory() as workspace:
            input_path = Path(workspace) / f"input.{SUFFIXES[format]}"
            input_path.write_bytes(document)
            markdown_path = Path(workspace) / "output.md"
            images_path = Path(workspace) / "images"

            completed = subprocess.run(
                [
                    str(self.binary),
                    str(input_path),
                    "-o",
                    str(markdown_path),
                    "--images",
                    str(images_path),
                ],
                capture_output=True,
                text=True,
                check=False,
            )
            if completed.returncode != 0:
                return Conversion(error=completed.stderr.strip())

            return Conversion(
                markdown=markdown_path.read_text(),
                images=_read_tree(images_path),
            )


def _read_tree(root: Path) -> dict[str, bytes]:
    if not root.is_dir():
        return {}
    return {
        path.relative_to(root).as_posix(): path.read_bytes()
        for path in sorted(root.rglob("*"))
        if path.is_file()
    }


FRONTENDS = {
    "cli": CliFrontend,
    "wasm": WasmFrontend,
}
