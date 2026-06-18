#!/usr/bin/env -S uv run --script
# /// script
# dependencies = ["mypy>=1.0.0", "ruff==0.14.2"]
# ///
"""Generate Rust ADK _gen type templates from genai_lambda_runtime sources.

The runtime package is the canonical input. This script intentionally does not
read from the Python ADK repository: it mirrors Python ADK's current stub sync
flow by running mypy's `stubgen`, post-processing the resulting `.pyi` files,
formatting them with Ruff, and using `assets/imports.json` as the public export
manifest. Extra support-only stubs are generated when public stubs import
sibling runtime modules, but they are not re-exported from `_gen`.

Generated project functions are executed in the PolyAI Lambda runtime, where the
real runtime modules are supplied by the platform. The checked-in `_gen` files
are for local editors and type checkers, not a replacement local runtime.
"""

from __future__ import annotations

import argparse
import ast
import filecmp
import json
import re
import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path

STUB_HEADER = "# Copyright PolyAI Limited\n"
INIT_HEADER = "# flake8: noqa\n# <AUTO GENERATED>\n"

LOCAL_OVERLAY_FILES = (
    "decorators.py",
    "integrations/__init__.py",
    "integrations/available_integrations/__init__.py",
)

UNRESOLVABLE_SOURCE_KEYS = frozenset({"utils/api_connector.py"})

_DROP_IMPORT_RE = re.compile(
    r"^from (?:_typeshed|constants|utils\.api_connector|utils\.secret_vault) .*\n",
    re.MULTILINE,
)
_INCOMPLETE_RE = re.compile(r"\bIncomplete\b")
_UNRESOLVABLE_TYPES_RE = re.compile(r"\bHandoffMethod\b|\bApiIntegrations\b")


@dataclass(frozen=True)
class SourceSpec:
    source_path: Path
    rel_path: Path
    exports: tuple[str, ...]


@dataclass(frozen=True)
class GeneratedModule:
    rel_path: Path
    module_name: str
    exports: tuple[str, ...]


def python_root_for_runtime(runtime_path: Path) -> Path:
    if runtime_path.name != "runtime":
        raise ValueError(f"runtime path must end with 'runtime': {runtime_path}")
    return runtime_path.parent


def stub_rel_path_from_imports_key(key: str) -> Path:
    parts = Path(key).parts
    if len(parts) < 2:
        raise ValueError(f"imports.json key must include a package prefix: {key}")
    if parts[0] not in {"runtime", "utils"}:
        raise ValueError(f"unsupported imports.json package prefix in key: {key}")
    return Path(*parts[1:]).with_suffix(".pyi")


def load_imports_json(python_root: Path) -> dict[str, list[str]]:
    imports_file = python_root / "assets" / "imports.json"
    if not imports_file.exists():
        raise FileNotFoundError(f"imports.json not found: {imports_file}")
    raw = json.loads(imports_file.read_text(encoding="utf-8"))
    if not isinstance(raw, dict):
        raise ValueError(f"imports.json must contain an object: {imports_file}")

    imports: dict[str, list[str]] = {}
    for key, names in raw.items():
        if not isinstance(key, str) or not isinstance(names, list):
            raise ValueError(f"invalid imports.json entry: {key!r}")
        if not all(isinstance(name, str) for name in names):
            raise ValueError(f"imports.json names must be strings for: {key}")
        imports[key] = names
    return imports


def imports_key_for_module(module: str) -> str | None:
    for prefix in ("runtime.", "utils."):
        if module.startswith(prefix):
            return module.replace(".", "/") + ".py"
    if module in {"runtime", "utils"}:
        return None
    return None


def dependency_keys_for_source(source_path: Path) -> set[str]:
    try:
        tree = ast.parse(source_path.read_text(encoding="utf-8"))
    except SyntaxError:
        return set()

    keys: set[str] = set()
    for stmt in tree.body:
        if isinstance(stmt, ast.Import):
            for alias in stmt.names:
                key = imports_key_for_module(alias.name)
                if key:
                    keys.add(key)
        elif isinstance(stmt, ast.ImportFrom) and stmt.module:
            key = imports_key_for_module(stmt.module)
            if key:
                keys.add(key)
            elif stmt.module in {"runtime", "utils"}:
                for alias in stmt.names:
                    keys.add(f"{stmt.module}/{alias.name}.py")
    return keys


def source_specs(runtime_path: Path) -> list[SourceSpec]:
    python_root = python_root_for_runtime(runtime_path)
    specs_by_key: dict[str, SourceSpec] = {}
    queue: list[str] = []
    for key, exports in sorted(load_imports_json(python_root).items()):
        source_path = python_root / key
        if not source_path.is_file():
            raise FileNotFoundError(f"imports.json references missing source file: {source_path}")
        specs_by_key[key] = SourceSpec(
            source_path=source_path,
            rel_path=stub_rel_path_from_imports_key(key),
            exports=tuple(exports),
        )
        queue.append(key)

    while queue:
        key = queue.pop(0)
        spec = specs_by_key[key]
        for dep_key in sorted(dependency_keys_for_source(spec.source_path)):
            if dep_key in specs_by_key or dep_key in UNRESOLVABLE_SOURCE_KEYS:
                continue
            source_path = python_root / dep_key
            if not source_path.is_file():
                continue
            specs_by_key[dep_key] = SourceSpec(
                source_path=source_path,
                rel_path=stub_rel_path_from_imports_key(dep_key),
                exports=(),
            )
            queue.append(dep_key)

    return [specs_by_key[key] for key in sorted(specs_by_key)]


def run_stubgen(specs: list[SourceSpec], output_dir: Path) -> None:
    cmd = [
        "stubgen",
        "-o",
        str(output_dir),
        *(str(spec.source_path) for spec in specs),
    ]
    result = subprocess.run(cmd, capture_output=True, text=True)
    if result.returncode != 0:
        print(result.stdout, file=sys.stdout, end="")
        print(result.stderr, file=sys.stderr, end="")
        print(
            "error: stubgen failed. Run this script with `uv run "
            "scripts/sync_runtime_gen_templates.py ...` so the inline mypy "
            "dependency is available.",
            file=sys.stderr,
        )
        raise SystemExit(result.returncode)


def run_ruff_format(output_dir: Path) -> None:
    cmd = [
        "ruff",
        "format",
        "--config",
        "line-length=100",
        "--config",
        "target-version='py312'",
        str(output_dir),
    ]
    result = subprocess.run(cmd, capture_output=True, text=True)
    if result.returncode != 0:
        print(result.stdout, file=sys.stdout, end="")
        print(result.stderr, file=sys.stderr, end="")
        print(
            "error: ruff format failed. Run this script with `uv run "
            "scripts/sync_runtime_gen_templates.py ...` so the inline ruff "
            "dependency is available.",
            file=sys.stderr,
        )
        raise SystemExit(result.returncode)


def path_has_suffix(path: Path, suffix_parts: tuple[str, ...]) -> bool:
    parts = path.parts
    return len(parts) >= len(suffix_parts) and parts[-len(suffix_parts) :] == suffix_parts


def find_generated_stub(stub_root: Path, spec: SourceSpec) -> Path:
    stub_name = spec.source_path.with_suffix(".pyi").name
    candidates = sorted(stub_root.rglob(stub_name))
    suffixes = [
        tuple(spec.source_path.with_suffix(".pyi").parts),
        ("runtime", *spec.rel_path.parts),
        ("utils", stub_name),
        tuple(spec.rel_path.parts),
    ]
    for suffix in suffixes:
        matches = [candidate for candidate in candidates if path_has_suffix(candidate, suffix)]
        if len(matches) == 1:
            return matches[0]
    if len(candidates) == 1:
        return candidates[0]
    raise FileNotFoundError(f"could not find stubgen output for {spec.source_path}")


def relative_module_name(current_package: tuple[str, ...], target_parts: tuple[str, ...]) -> str:
    common = 0
    for current_part, target_part in zip(current_package, target_parts):
        if current_part != target_part:
            break
        common += 1

    up_levels = len(current_package) - common
    dots = "." * (up_levels + 1)
    remainder = ".".join(target_parts[common:])
    return f"{dots}{remainder}" if remainder else dots


def runtime_relative_module(module: str, current_package: tuple[str, ...]) -> str | None:
    for prefix in ("runtime.", "utils."):
        if module.startswith(prefix):
            target_parts = tuple(module[len(prefix) :].split("."))
            return relative_module_name(current_package, target_parts)
    if module in {"runtime", "utils"}:
        return "."
    return None


def render_import_from(node: ast.ImportFrom) -> str | None:
    names = ", ".join(
        f"{alias.name} as {alias.asname}" if alias.asname else alias.name for alias in node.names
    )
    if not names:
        return None
    prefix = "." * node.level
    module = node.module or ""
    return f"from {prefix}{module} import {names}"


def rewrite_imports(source: str, rel_path: Path) -> str:
    current_package = rel_path.with_suffix("").parts[:-1]
    output_lines: list[str] = []

    for line in source.splitlines():
        stripped = line.strip()
        try:
            parsed = ast.parse(stripped).body[0] if stripped else None
        except SyntaxError:
            parsed = None

        if isinstance(parsed, ast.Import):
            rewritten: list[str] = []
            kept: list[ast.alias] = []
            for alias in parsed.names:
                runtime_module = runtime_relative_module(alias.name, current_package)
                if runtime_module is None:
                    kept.append(alias)
                    continue
                target_parts = tuple(alias.name.split(".")[1:])
                imported_name = target_parts[-1] if target_parts else alias.name
                parent_module = relative_module_name(current_package, target_parts[:-1])
                alias_part = f" as {alias.asname}" if alias.asname else ""
                rewritten.append(f"from {parent_module} import {imported_name}{alias_part}")
            output_lines.extend(rewritten)
            if kept:
                output_lines.append(ast.unparse(ast.Import(names=kept)))
            continue

        if isinstance(parsed, ast.ImportFrom) and parsed.module:
            runtime_module = runtime_relative_module(parsed.module, current_package)
            if runtime_module is not None:
                rendered = render_import_from(
                    ast.ImportFrom(
                        module=runtime_module.lstrip("."),
                        names=parsed.names,
                        level=len(runtime_module) - len(runtime_module.lstrip(".")),
                    )
                )
                if rendered:
                    output_lines.append(rendered)
                continue

        output_lines.append(line)

    return "\n".join(output_lines).rstrip() + "\n"


def ensure_any_imported(source: str) -> str:
    if "from typing import" in source:
        return re.sub(
            r"from typing import (.+)",
            lambda m: f"from typing import {m.group(1)}"
            if "Any" in m.group(1).split(", ")
            else f"from typing import Any, {m.group(1)}",
            source,
            count=1,
        )
    return "from typing import Any\n" + source


def apply_ty_compatibility_fixes(source: str, rel_path: Path) -> str:
    if "dict[str, any]" in source:
        source = source.replace("dict[str, any]", "dict[str, Any]")
        source = ensure_any_imported(source)
    if " = None" in source:
        source = re.sub(r": str = None", ": str | None = None", source)
    source = source.replace(
        "def __readonly__(self, *args, **kwargs) -> None: ...",
        "def __readonly__(self, *args, **kwargs) -> Any: ...",
    )
    source = source.replace("from pydantic import BaseModel\n", "class BaseModel:\n    ...\n")

    if rel_path == Path("integrations/integration.pyi") and "_registry" not in source:
        lines = source.splitlines()
        insert_at = 0
        while insert_at < len(lines) and (
            lines[insert_at].startswith("__all__")
            or lines[insert_at].startswith("import ")
            or lines[insert_at].startswith("from ")
            or not lines[insert_at].strip()
        ):
            insert_at += 1
        lines.insert(insert_at, "_registry: dict[str, type[Integration]]")
        source = "\n".join(lines).rstrip() + "\n"

    return source


def assignment_names(stmt: ast.Assign | ast.AnnAssign) -> list[str]:
    if isinstance(stmt, ast.Assign):
        return [target.id for target in stmt.targets if isinstance(target, ast.Name)]
    if isinstance(stmt.target, ast.Name):
        return [stmt.target.id]
    return []


def available_bound_names(source: str) -> set[str]:
    tree = ast.parse(source)
    names: set[str] = set()
    for node in ast.iter_child_nodes(tree):
        if isinstance(node, (ast.ClassDef, ast.FunctionDef, ast.AsyncFunctionDef)):
            names.add(node.name)
        elif isinstance(node, (ast.Assign, ast.AnnAssign)):
            names.update(assignment_names(node))
        elif isinstance(node, (ast.Import, ast.ImportFrom)):
            names.update(alias.asname or alias.name for alias in node.names)
    return names


def postprocess_stub(stub_source: str, spec: SourceSpec) -> tuple[str, tuple[str, ...]]:
    source = _DROP_IMPORT_RE.sub("", stub_source)

    needs_any = False
    for pattern in (_INCOMPLETE_RE, _UNRESOLVABLE_TYPES_RE):
        if pattern.search(source):
            source = pattern.sub("Any", source)
            needs_any = True
    if needs_any:
        source = ensure_any_imported(source)

    source = rewrite_imports(source, spec.rel_path)
    source = apply_ty_compatibility_fixes(source, spec.rel_path)
    available_names = available_bound_names(source)
    exports = tuple(name for name in spec.exports if name in available_names)
    if exports:
        source = "__all__ = " + repr(list(exports)) + "\n\n" + source
    header = STUB_HEADER
    if spec.rel_path == Path("integrations/available_integrations/opentable.pyi"):
        header += "# ty: ignore\n"
    return header + source.lstrip(), exports


def read_all_from_stub(path: Path) -> tuple[str, ...]:
    try:
        tree = ast.parse(path.read_text(encoding="utf-8"))
    except SyntaxError:
        return ()
    for node in ast.iter_child_nodes(tree):
        if not isinstance(node, ast.Assign):
            continue
        for target in node.targets:
            if not isinstance(target, ast.Name) or target.id != "__all__":
                continue
            if isinstance(node.value, (ast.List, ast.Tuple)):
                return tuple(
                    element.value
                    for element in node.value.elts
                    if isinstance(element, ast.Constant) and isinstance(element.value, str)
                )
    return ()


def module_name_for_path(path: Path) -> str:
    return ".".join(path.with_suffix("").parts)


def generate_root_init(modules: list[GeneratedModule], decorator_exports: tuple[str, ...]) -> str:
    export_entries: list[str] = []
    import_blocks: list[str] = []

    for module in sorted(modules, key=lambda item: item.module_name):
        if module.rel_path.name == "__init__.py" or not module.exports:
            continue
        export_entries.extend(module.exports)
        import_blocks.append(
            "from _gen."
            + module.module_name
            + " import (\n    "
            + ", ".join(module.exports)
            + "\n)"
        )

    if decorator_exports:
        export_entries.extend(decorator_exports)
        import_blocks.append("from _gen.decorators import " + ", ".join(decorator_exports))

    all_block = "__all__ = [\n"
    if export_entries:
        all_block += "    " + ",\n    ".join(f'"{name}"' for name in export_entries) + "\n"
    all_block += "]\n\n"

    return INIT_HEADER + all_block + "\n".join(import_blocks) + ("\n" if import_blocks else "")


def copy_local_overlays(template_dir: Path, generated_dir: Path) -> tuple[str, ...]:
    decorator_exports: tuple[str, ...] = ()
    for rel in LOCAL_OVERLAY_FILES:
        source = template_dir / rel
        if not source.exists():
            continue
        dest = generated_dir / rel
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, dest)
        if rel == "decorators.py":
            decorator_exports = read_all_from_stub(dest)
    return decorator_exports


def generate_tree(runtime_path: Path, output_dir: Path, template_dir: Path) -> None:
    if output_dir.exists():
        shutil.rmtree(output_dir)
    output_dir.mkdir(parents=True)

    specs = source_specs(runtime_path)
    modules: list[GeneratedModule] = []

    with tempfile.TemporaryDirectory(prefix="adk-stubgen-") as temp:
        stubgen_dir = Path(temp) / "stubgen"
        run_stubgen(specs, stubgen_dir)

        for spec in specs:
            stub_path = find_generated_stub(stubgen_dir, spec)
            generated_source, exports = postprocess_stub(
                stub_path.read_text(encoding="utf-8"),
                spec,
            )

            dest = output_dir / spec.rel_path
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_text(generated_source, encoding="utf-8")
            modules.append(
                GeneratedModule(
                    rel_path=spec.rel_path,
                    module_name=module_name_for_path(spec.rel_path),
                    exports=exports,
                )
            )

    decorator_exports = copy_local_overlays(template_dir, output_dir)
    root_init = generate_root_init(modules, decorator_exports)
    (output_dir / "__init__.py").write_text(root_init, encoding="utf-8")
    run_ruff_format(output_dir)


def all_regular_files(root: Path) -> set[Path]:
    if not root.exists():
        return set()
    return {
        path.relative_to(root)
        for path in root.rglob("*")
        if path.is_file() and "__pycache__" not in path.parts
    }


def compare_trees(expected: Path, actual: Path) -> list[str]:
    expected_files = all_regular_files(expected)
    actual_files = all_regular_files(actual)
    messages: list[str] = []

    for rel in sorted(expected_files - actual_files):
        messages.append(f"missing: {rel}")
    for rel in sorted(actual_files - expected_files):
        messages.append(f"extra: {rel}")
    for rel in sorted(expected_files & actual_files):
        if not filecmp.cmp(expected / rel, actual / rel, shallow=False):
            messages.append(f"changed: {rel}")
    return messages


def copy_generated_tree(generated_dir: Path, output_dir: Path) -> None:
    if output_dir.exists():
        shutil.rmtree(output_dir)
    shutil.copytree(generated_dir, output_dir)


def sync_fixture(generated_dir: Path, fixture_gen_dir: Path) -> None:
    if fixture_gen_dir.exists():
        shutil.rmtree(fixture_gen_dir)
    shutil.copytree(generated_dir, fixture_gen_dir)


def main() -> int:
    repo_root = Path(__file__).resolve().parent.parent
    default_runtime = repo_root.parent / "genai_lambda_runtime" / "python" / "runtime"
    default_template_dir = repo_root / "adk-core" / "python-gen-template"

    parser = argparse.ArgumentParser(description="Sync Rust ADK _gen templates from runtime sources")
    parser.add_argument(
        "--runtime-path",
        type=Path,
        default=default_runtime,
        help="Path to genai_lambda_runtime/python/runtime",
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=default_template_dir,
        help="Template directory to update",
    )
    parser.add_argument(
        "--overlay-dir",
        type=Path,
        default=default_template_dir,
        help="Directory containing Rust-owned overlay templates such as decorators.py",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="Fail if generated templates differ from --output-dir",
    )
    parser.add_argument(
        "--sync-fixture",
        type=Path,
        help="Also replace a fixture _gen directory with the generated tree",
    )
    args = parser.parse_args()

    runtime_path = args.runtime_path.resolve()
    output_dir = args.output_dir.resolve()
    overlay_dir = args.overlay_dir.resolve()
    if not runtime_path.is_dir():
        print(f"error: runtime path not found: {runtime_path}", file=sys.stderr)
        return 2
    if not overlay_dir.is_dir():
        print(f"error: overlay directory not found: {overlay_dir}", file=sys.stderr)
        return 2

    with tempfile.TemporaryDirectory(prefix="adk-runtime-stubs-") as temp:
        generated_dir = Path(temp) / "python-gen-template"
        generate_tree(runtime_path, generated_dir, overlay_dir)

        if args.check:
            differences = compare_trees(generated_dir, output_dir)
            if differences:
                print("generated templates differ from checked-in templates:", file=sys.stderr)
                for difference in differences:
                    print(f"  {difference}", file=sys.stderr)
                return 1
            print(f"OK: {output_dir} is up to date")
            return 0

        copy_generated_tree(generated_dir, output_dir)
        print(f"synced templates to {output_dir}")

        if args.sync_fixture:
            sync_fixture(generated_dir, args.sync_fixture.resolve())
            print(f"synced fixture _gen to {args.sync_fixture.resolve()}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
