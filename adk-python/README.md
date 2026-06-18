# poly_adk

Python bindings for the PolyAI ADK Rust service layer.

This package exposes a Pythonic scripting surface over an existing ADK project:

```python
from poly_adk import Project

project = Project.open(".")
print(project.status().modified_files)
```

Merge conflict resolutions are typed before they cross into Rust:

```python
from poly_adk import MergeResolution, Project

project = Project.open(".")
project.branches.merge(
    "Merge feature branch",
    resolutions=[
        MergeResolution(["flows", "support", "prompt"], "theirs"),
    ],
)
```

## Local development

Run the Python smoke tests from this directory with:

```bash
uv run python -m unittest discover -s tests
```

`uv run` installs this Maturin project into its managed environment using the
PEP 660 editable-install support configured in `pyproject.toml`. The `dev`
dependency group includes `maturin-import-hook`, which rebuilds the native
extension when Rust sources have changed before the tests import `poly_adk`.
