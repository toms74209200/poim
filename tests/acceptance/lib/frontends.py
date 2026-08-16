import struct
from dataclasses import dataclass, field
from pathlib import Path

from wasmtime import Instance, Module, Store

STATUS_OK = 0
FORMAT_EPUB = 0


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

    def convert(self, epub: bytes) -> Conversion:
        pointer = self.exports["alloc"](self.store, len(epub))
        assert pointer != 0, "alloc returned a null pointer"
        memory = self.exports["memory"]
        memory.write(self.store, epub, pointer)

        result = self.exports["convert"](self.store, pointer, len(epub), FORMAT_EPUB)
        self.exports["free"](self.store, pointer, len(epub))

        total = struct.unpack_from("<I", memory.read(self.store, result, result + 4))[0]
        packed = bytes(memory.read(self.store, result, result + total))
        self.exports["free"](self.store, result, total)
        return unpack(packed)


FRONTENDS = {
    "wasm": WasmFrontend,
}
