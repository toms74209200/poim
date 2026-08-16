from pathlib import Path

import pytest

from lib import reference_epub
from lib.frontends import FRONTENDS

REPO_ROOT = Path(__file__).resolve().parents[2]

_FRONTEND_OPTION = "--frontend"


def pytest_addoption(parser: pytest.Parser) -> None:
    parser.addoption(
        _FRONTEND_OPTION,
        action="store",
        default=None,
        help=f"frontend under test: {', '.join(FRONTENDS)}",
    )


@pytest.fixture(scope="session")
def frontend(request):
    name = request.config.getoption(_FRONTEND_OPTION)
    assert name, f"{_FRONTEND_OPTION} is required, one of: {', '.join(FRONTENDS)}"
    assert name in FRONTENDS, (
        f"unknown frontend {name!r}, expected one of: {', '.join(FRONTENDS)}"
    )
    return FRONTENDS[name](REPO_ROOT)


@pytest.fixture(scope="session")
def reference_epub_path() -> Path:
    return reference_epub.fetch()


@pytest.fixture
def epub() -> dict:
    return {}


@pytest.fixture
def result() -> dict:
    return {}
