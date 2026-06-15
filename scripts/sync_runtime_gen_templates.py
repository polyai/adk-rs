#!/usr/bin/env -S uv run --script
# /// script
# dependencies = ["mypy>=1.0.0"]
# ///
"""Generate Rust ADK _gen templates from genai_lambda_runtime sources.

The runtime package is the canonical input. This script intentionally does not
read from the Python ADK repository: it shells out to mypy's `stubgen` for
signature extraction, then post-processes the generated `.pyi` files into
runtime-importable `.py` helper modules used by `poly init` and `poly pull`.
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

STUB_HEADER = """\
# Copyright PolyAI Limited
# flake8: noqa
# ruff: noqa
# type: ignore
from __future__ import annotations
"""

INIT_HEADER = "# flake8: noqa\n# <AUTO GENERATED>\n"

LOCAL_OVERLAY_FILES = (
    "decorators.py",
    "secret_vault.py",
)

SKIPPED_MODULES = frozenset(
    {
        "state_utils",
        "llm_client",
        "decorators",
        "secret_vault",
        "utils.secret_vault",
    }
)

TYPE_ONLY_IMPORT_ROOTS = frozenset({"constants", "requests"})
OVERLAY_KEYS = frozenset({"runtime/decorators.py", "utils/secret_vault.py"})
UNRESOLVABLE_TYPE_NAMES = frozenset({"ApiIntegrations", "HandoffMethod"})
UNRESOLVABLE_SOURCE_KEYS = frozenset({"utils/api_connector.py"})
PACKAGE_INIT_EXPORTS = {
    "runtime/integrations/__init__.py": ("Integration", "registry"),
}


@dataclass(frozen=True)
class SourceSpec:
    source_path: Path
    rel_path: Path
    exports: tuple[str, ...]
    filter_names: frozenset[str] | None = None


@dataclass(frozen=True)
class GeneratedModule:
    path: Path
    module_name: str
    exports: tuple[str, ...]


def has_private_name(name: str) -> bool:
    return name.startswith("_") and not (name.startswith("__") and name.endswith("__"))


def module_name_for_path(runtime_path: Path, source_path: Path) -> str:
    rel = source_path.relative_to(runtime_path).with_suffix("")
    return ".".join(rel.parts)


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
    return Path(*parts[1:])


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


def add_dependency_specs(
    python_root: Path,
    specs_by_key: dict[str, SourceSpec],
    queue: list[str],
) -> None:
    while queue:
        key = queue.pop(0)
        spec = specs_by_key[key]

        for dep_key in sorted(dependency_keys_for_source(spec.source_path)):
            if dep_key in specs_by_key or dep_key in OVERLAY_KEYS or dep_key in UNRESOLVABLE_SOURCE_KEYS:
                continue
            source_path = python_root / dep_key
            if not source_path.is_file():
                continue
            dep_spec = SourceSpec(
                source_path=source_path,
                rel_path=stub_rel_path_from_imports_key(dep_key),
                exports=(),
            )
            specs_by_key[dep_key] = dep_spec
            queue.append(dep_key)


def source_specs(runtime_path: Path) -> list[SourceSpec]:
    python_root = python_root_for_runtime(runtime_path)
    specs_by_key: dict[str, SourceSpec] = {}
    queue: list[str] = []
    for key, exports in sorted(load_imports_json(python_root).items()):
        if key in OVERLAY_KEYS:
            continue

        source_path = python_root / key
        if not source_path.is_file():
            raise FileNotFoundError(f"imports.json references missing source file: {source_path}")
        specs_by_key[key] = SourceSpec(
            source_path=source_path,
            rel_path=stub_rel_path_from_imports_key(key),
            exports=tuple(exports),
        )
        queue.append(key)

    for key, exports in PACKAGE_INIT_EXPORTS.items():
        source_path = python_root / key
        if source_path.is_file():
            specs_by_key[key] = SourceSpec(
                source_path=source_path,
                rel_path=stub_rel_path_from_imports_key(key),
                exports=exports,
            )
            queue.append(key)

    add_dependency_specs(python_root, specs_by_key, queue)
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


def path_has_suffix(path: Path, suffix_parts: tuple[str, ...]) -> bool:
    parts = path.parts
    return len(parts) >= len(suffix_parts) and parts[-len(suffix_parts) :] == suffix_parts


def find_generated_stub(stub_root: Path, spec: SourceSpec) -> Path:
    candidates = sorted(stub_root.rglob(spec.source_path.with_suffix(".pyi").name))
    suffixes = [
        tuple(spec.source_path.with_suffix(".pyi").parts),
        ("runtime", *spec.rel_path.with_suffix(".pyi").parts),
        ("utils", spec.source_path.with_suffix(".pyi").name),
        tuple(spec.rel_path.with_suffix(".pyi").parts),
    ]
    for suffix in suffixes:
        matches = [candidate for candidate in candidates if path_has_suffix(candidate, suffix)]
        if len(matches) == 1:
            return matches[0]
    if len(candidates) == 1:
        return candidates[0]
    raise FileNotFoundError(f"could not find stubgen output for {spec.source_path}")


def current_package_for_rel_path(rel_path: Path) -> tuple[str, ...]:
    return rel_path.with_suffix("").parts[:-1]


def relative_module_name(current_package: tuple[str, ...], target_parts: tuple[str, ...]) -> str:
    common = 0
    for current_part, target_part in zip(current_package, target_parts):
        if current_part != target_part:
            break
        common += 1

    up_levels = len(current_package) - common
    level = "." * (up_levels + 1)
    remainder = ".".join(target_parts[common:])
    return f"{level}{remainder}" if remainder else level


def runtime_relative_module(module: str, current_package: tuple[str, ...]) -> str | None:
    for prefix in ("runtime.", "utils."):
        if module.startswith(prefix):
            target_parts = tuple(module[len(prefix) :].split("."))
            return relative_module_name(current_package, target_parts)
    if module in {"runtime", "utils"}:
        return "."
    return None


def import_root(module: str) -> str:
    return module.split(".", 1)[0]


def is_type_only_import(module: str) -> bool:
    return import_root(module) in TYPE_ONLY_IMPORT_ROOTS


def render_import_from(node: ast.ImportFrom) -> str | None:
    names = ", ".join(
        f"{alias.name} as {alias.asname}" if alias.asname else alias.name
        for alias in node.names
    )
    if not names:
        return None
    prefix = "." * node.level
    module = node.module or ""
    return f"from {prefix}{module} import {names}"


def rewrite_imports(source: str, rel_path: Path) -> str:
    current_package = current_package_for_rel_path(rel_path)
    type_only_lines: list[str] = []
    output_lines: list[str] = []

    for line in source.splitlines():
        stripped = line.strip()
        try:
            parsed = ast.parse(stripped).body[0] if stripped else None
        except SyntaxError:
            parsed = None

        if isinstance(parsed, ast.Import):
            runtime_aliases: list[str] = []
            type_only_aliases: list[ast.alias] = []
            kept_aliases: list[ast.alias] = []
            for alias in parsed.names:
                runtime_module = runtime_relative_module(alias.name, current_package)
                if runtime_module is not None:
                    target_parts = tuple(alias.name.split(".")[1:])
                    imported_name = target_parts[-1] if target_parts else alias.name
                    parent_module = relative_module_name(current_package, target_parts[:-1])
                    alias_part = f" as {alias.asname}" if alias.asname else ""
                    runtime_aliases.append(f"from {parent_module} import {imported_name}{alias_part}")
                elif is_type_only_import(alias.name):
                    type_only_aliases.append(alias)
                else:
                    kept_aliases.append(alias)
            output_lines.extend(runtime_aliases)
            if type_only_aliases:
                type_only_lines.append(ast.unparse(ast.Import(names=type_only_aliases)))
            if kept_aliases:
                output_lines.append(ast.unparse(ast.Import(names=kept_aliases)))
            continue

        if isinstance(parsed, ast.ImportFrom) and parsed.module:
            runtime_module = runtime_relative_module(parsed.module, current_package)
            if runtime_module is not None:
                rendered = render_import_from(
                    ast.ImportFrom(module=runtime_module.lstrip("."), names=parsed.names, level=len(runtime_module) - len(runtime_module.lstrip(".")))
                )
                if rendered:
                    output_lines.append(rendered)
                continue
            if is_type_only_import(parsed.module):
                type_only_lines.append(stripped)
                continue

        output_lines.append(line)

    if type_only_lines:
        output_lines = ensure_typing_import(output_lines, {"TYPE_CHECKING"})
        output_lines.extend(["", "if TYPE_CHECKING:"])
        output_lines.extend(f"    {line}" for line in dict.fromkeys(type_only_lines))

    return "\n".join(output_lines).rstrip() + "\n"


def ensure_typing_import(lines: list[str], names: set[str]) -> list[str]:
    for index, line in enumerate(lines):
        if line.startswith("from typing import "):
            existing = [part.strip() for part in line.removeprefix("from typing import ").split(",")]
            merged = sorted(set(existing) | names)
            lines[index] = "from typing import " + ", ".join(merged)
            return lines

    insert_at = 0
    while insert_at < len(lines) and (
        lines[insert_at].startswith("#") or lines[insert_at] == "" or lines[insert_at].startswith("from __future__")
    ):
        insert_at += 1
    lines.insert(insert_at, "from typing import " + ", ".join(sorted(names)))
    return lines


def replace_incomplete_with_any(source: str) -> str:
    if "Incomplete" not in source:
        return source
    source = re.sub(r"^from _typeshed import Incomplete\n", "", source, flags=re.MULTILINE)
    source = re.sub(r"\bIncomplete\b", "Any", source)
    lines = ensure_typing_import(source.splitlines(), {"Any"})
    return "\n".join(lines).rstrip() + "\n"


def apply_pydantic_fallback(source: str) -> str:
    pattern = re.compile(r"^from pydantic import ([^\n]+)$", re.MULTILINE)

    def replace(match: re.Match[str]) -> str:
        names = [part.strip() for part in match.group(1).split(",")]
        if "BaseModel" not in names:
            return match.group(0)
        remaining = [name for name in names if name != "BaseModel"]
        pieces = []
        if remaining:
            pieces.append("from pydantic import " + ", ".join(remaining))
        pieces.extend(
            [
                "try:",
                "    from pydantic import BaseModel",
                "except ImportError:",
                "    class BaseModel:",
                "        ...",
            ]
        )
        return "\n".join(pieces)

    return pattern.sub(replace, source)


def get_source_all(tree: ast.Module) -> list[str] | None:
    for stmt in tree.body:
        if not isinstance(stmt, ast.Assign):
            continue
        for target in stmt.targets:
            if not isinstance(target, ast.Name) or target.id != "__all__":
                continue
            if isinstance(stmt.value, (ast.List, ast.Tuple)):
                return [
                    element.value
                    for element in stmt.value.elts
                    if isinstance(element, ast.Constant) and isinstance(element.value, str)
                ]
    return None


def public_names_from_tree(tree: ast.Module) -> tuple[str, ...]:
    names: list[str] = []
    for stmt in tree.body:
        if isinstance(stmt, (ast.ClassDef, ast.FunctionDef, ast.AsyncFunctionDef)):
            if not has_private_name(stmt.name):
                names.append(stmt.name)
        elif isinstance(stmt, (ast.Assign, ast.AnnAssign)):
            for name in assignment_names(stmt):
                if name != "__all__" and not has_private_name(name):
                    names.append(name)
    return tuple(dict.fromkeys(names))


def assignment_names(stmt: ast.Assign | ast.AnnAssign) -> list[str]:
    if isinstance(stmt, ast.Assign):
        return [target.id for target in stmt.targets if isinstance(target, ast.Name)]
    if isinstance(stmt.target, ast.Name):
        return [stmt.target.id]
    return []


def filter_top_level_names(source: str, names: frozenset[str] | None) -> str:
    if not names:
        return source
    tree = ast.parse(source)
    lines = source.splitlines()
    keep = [True] * len(lines)
    for stmt in tree.body:
        stmt_names: list[str] = []
        if isinstance(stmt, (ast.ClassDef, ast.FunctionDef, ast.AsyncFunctionDef)):
            stmt_names = [stmt.name]
        elif isinstance(stmt, (ast.Assign, ast.AnnAssign)):
            stmt_names = assignment_names(stmt)
        else:
            continue
        if stmt_names and not any(name in names for name in stmt_names):
            start_lineno = stmt.lineno
            if isinstance(stmt, (ast.ClassDef, ast.FunctionDef, ast.AsyncFunctionDef)) and stmt.decorator_list:
                start_lineno = min(decorator.lineno for decorator in stmt.decorator_list)
            for index in range(start_lineno - 1, stmt.end_lineno or stmt.lineno):
                keep[index] = False
    return "\n".join(line for line, should_keep in zip(lines, keep) if should_keep).rstrip() + "\n"


def name_used_in(name: str, source: str) -> bool:
    return bool(re.search(rf"(?<![A-Za-z0-9_]){re.escape(name)}(?![A-Za-z0-9_])", source))


def alias_bound_name(alias: ast.alias) -> str:
    return alias.asname or alias.name.split(".", 1)[0]


def prune_unused_imports(source: str) -> str:
    tree = ast.parse(source)
    lines = source.splitlines()
    output = lines[:]
    import_ranges: list[tuple[int, int]] = []
    for stmt in tree.body:
        if isinstance(stmt, (ast.Import, ast.ImportFrom)) and not (
            isinstance(stmt, ast.ImportFrom) and stmt.module == "__future__"
        ):
            import_ranges.append((stmt.lineno - 1, stmt.end_lineno or stmt.lineno))

    body_lines = [
        line
        for index, line in enumerate(lines)
        if not any(start <= index < end for start, end in import_ranges)
    ]
    body_text = "\n".join(body_lines)

    for stmt in ast.walk(tree):
        if not isinstance(stmt, (ast.Import, ast.ImportFrom)):
            continue
        if isinstance(stmt, ast.ImportFrom) and stmt.module == "__future__":
            continue

        kept_aliases = [alias for alias in stmt.names if name_used_in(alias_bound_name(alias), body_text)]
        if len(kept_aliases) == len(stmt.names):
            continue

        if kept_aliases:
            if isinstance(stmt, ast.Import):
                replacement = ast.unparse(ast.Import(names=kept_aliases))
            else:
                replacement = ast.unparse(
                    ast.ImportFrom(module=stmt.module, names=kept_aliases, level=stmt.level)
                )
            output[stmt.lineno - 1] = replacement
            for index in range(stmt.lineno, stmt.end_lineno or stmt.lineno):
                output[index] = ""
        else:
            for index in range(stmt.lineno - 1, stmt.end_lineno or stmt.lineno):
                output[index] = ""

    return "\n".join(output).rstrip() + "\n"


def replace_unresolvable_types(source: str) -> str:
    if not any(name_used_in(name, source) for name in UNRESOLVABLE_TYPE_NAMES):
        return source

    tree = ast.parse(source)
    lines = source.splitlines()
    output = lines[:]

    for stmt in tree.body:
        if not isinstance(stmt, (ast.Import, ast.ImportFrom)):
            continue
        if isinstance(stmt, ast.ImportFrom) and stmt.module == "__future__":
            continue

        kept_aliases = [
            alias for alias in stmt.names if alias_bound_name(alias) not in UNRESOLVABLE_TYPE_NAMES
        ]
        if len(kept_aliases) == len(stmt.names):
            continue

        if kept_aliases:
            if isinstance(stmt, ast.Import):
                replacement = ast.unparse(ast.Import(names=kept_aliases))
            else:
                replacement = ast.unparse(
                    ast.ImportFrom(module=stmt.module, names=kept_aliases, level=stmt.level)
                )
            output[stmt.lineno - 1] = replacement
            for index in range(stmt.lineno, stmt.end_lineno or stmt.lineno):
                output[index] = ""
        else:
            for index in range(stmt.lineno - 1, stmt.end_lineno or stmt.lineno):
                output[index] = ""

    replaced = "\n".join(output)
    for name in UNRESOLVABLE_TYPE_NAMES:
        replaced = re.sub(rf"(?<![A-Za-z0-9_]){re.escape(name)}(?![A-Za-z0-9_])", "Any", replaced)
    replaced = re.sub(r"^\s*from constants import Any as Any\n", "", replaced, flags=re.MULTILINE)
    return "\n".join(ensure_typing_import(replaced.splitlines(), {"Any"})).rstrip() + "\n"


def is_type_checking_guard(stmt: ast.stmt) -> bool:
    return (
        isinstance(stmt, ast.If)
        and isinstance(stmt.test, ast.Name)
        and stmt.test.id == "TYPE_CHECKING"
    )


def prune_type_checking_imports(source: str) -> str:
    tree = ast.parse(source)
    lines = source.splitlines()
    output = lines[:]
    import_ranges: list[tuple[int, int]] = []
    guarded_imports: list[ast.Import | ast.ImportFrom] = []
    guards: list[ast.If] = []

    for stmt in tree.body:
        if not is_type_checking_guard(stmt):
            continue
        guards.append(stmt)
        for child in stmt.body:
            if isinstance(child, (ast.Import, ast.ImportFrom)):
                guarded_imports.append(child)
                import_ranges.append((child.lineno - 1, child.end_lineno or child.lineno))

    if not guarded_imports:
        return source

    body_lines = [
        line
        for index, line in enumerate(lines)
        if not any(start <= index < end for start, end in import_ranges)
    ]
    body_text = "\n".join(body_lines)

    for stmt in guarded_imports:
        kept_aliases = [alias for alias in stmt.names if name_used_in(alias_bound_name(alias), body_text)]
        if len(kept_aliases) == len(stmt.names):
            continue

        if kept_aliases:
            if isinstance(stmt, ast.Import):
                replacement = ast.unparse(ast.Import(names=kept_aliases))
            else:
                replacement = ast.unparse(
                    ast.ImportFrom(module=stmt.module, names=kept_aliases, level=stmt.level)
                )
            output[stmt.lineno - 1] = "    " + replacement
            for index in range(stmt.lineno, stmt.end_lineno or stmt.lineno):
                output[index] = ""
        else:
            for index in range(stmt.lineno - 1, stmt.end_lineno or stmt.lineno):
                output[index] = ""

    for guard in guards:
        body = output[guard.lineno : guard.end_lineno or guard.lineno]
        if not any(line.strip() for line in body):
            for index in range(guard.lineno - 1, guard.end_lineno or guard.lineno):
                output[index] = ""

    return "\n".join(output).rstrip() + "\n"


def source_type_checking_bound_names(source: str) -> set[str]:
    try:
        tree = ast.parse(source)
    except SyntaxError:
        return set()

    names: set[str] = set()
    for stmt in tree.body:
        if not is_type_checking_guard(stmt):
            continue
        for child in stmt.body:
            if isinstance(child, (ast.Import, ast.ImportFrom)):
                names.update(alias_bound_name(alias) for alias in child.names)
    return names


def move_named_imports_to_type_checking(source: str, names: set[str]) -> str:
    if not names:
        return source

    tree = ast.parse(source)
    lines = source.splitlines()
    output = lines[:]
    type_only_lines: list[str] = []

    for stmt in tree.body:
        if not isinstance(stmt, (ast.Import, ast.ImportFrom)):
            continue
        if isinstance(stmt, ast.ImportFrom) and stmt.module == "__future__":
            continue

        type_only_aliases = [alias for alias in stmt.names if alias_bound_name(alias) in names]
        if not type_only_aliases:
            continue

        kept_aliases = [alias for alias in stmt.names if alias_bound_name(alias) not in names]
        if isinstance(stmt, ast.Import):
            type_only_lines.append(ast.unparse(ast.Import(names=type_only_aliases)))
            replacement = ast.unparse(ast.Import(names=kept_aliases)) if kept_aliases else None
        else:
            type_only_lines.append(
                ast.unparse(ast.ImportFrom(module=stmt.module, names=type_only_aliases, level=stmt.level))
            )
            replacement = (
                ast.unparse(ast.ImportFrom(module=stmt.module, names=kept_aliases, level=stmt.level))
                if kept_aliases
                else None
            )

        if replacement:
            output[stmt.lineno - 1] = replacement
            for index in range(stmt.lineno, stmt.end_lineno or stmt.lineno):
                output[index] = ""
        else:
            for index in range(stmt.lineno - 1, stmt.end_lineno or stmt.lineno):
                output[index] = ""

    if not type_only_lines:
        return source

    output = ensure_typing_import(output, {"TYPE_CHECKING"})
    output.extend(["", "if TYPE_CHECKING:"])
    output.extend(f"    {line}" for line in dict.fromkeys(type_only_lines))
    return "\n".join(output).rstrip() + "\n"


def collapse_blank_lines(source: str, max_run: int = 1) -> str:
    output: list[str] = []
    blank_run = 0
    for line in source.splitlines():
        if line:
            blank_run = 0
            output.append(line)
        else:
            blank_run += 1
            if blank_run <= max_run:
                output.append(line)
    return "\n".join(output).rstrip() + "\n"


def simple_class_assignment(stmt: ast.Assign) -> str | None:
    if len(stmt.targets) != 1 or not isinstance(stmt.targets[0], ast.Name):
        return None
    name = stmt.targets[0].id
    if has_private_name(name) or not is_simple_assignment_value(stmt.value):
        return None
    return f"{name} = {ast.unparse(stmt.value)}"


def is_simple_assignment_value(node: ast.expr) -> bool:
    if isinstance(node, ast.Constant):
        return True
    if isinstance(node, ast.Name):
        return True
    if isinstance(node, ast.UnaryOp):
        return is_simple_assignment_value(node.operand)
    if isinstance(node, ast.BinOp) and isinstance(node.op, ast.BitOr):
        return is_simple_assignment_value(node.left) and is_simple_assignment_value(node.right)
    if isinstance(node, ast.Subscript):
        return is_simple_assignment_value(node.value) and is_simple_assignment_value(node.slice)
    if isinstance(node, ast.Slice):
        return all(
            part is None or is_simple_assignment_value(part)
            for part in (node.lower, node.upper, node.step)
        )
    if isinstance(node, (ast.List, ast.Tuple, ast.Set)):
        return all(is_simple_assignment_value(element) for element in node.elts)
    if isinstance(node, ast.Dict):
        return all(
            key is not None and is_simple_assignment_value(key) and is_simple_assignment_value(value)
            for key, value in zip(node.keys, node.values)
        )
    if (
        isinstance(node, ast.Call)
        and isinstance(node.func, ast.Name)
        and node.func.id in {"cast", "NewType"}
        and len(node.args) == 2
        and not node.keywords
    ):
        return is_simple_assignment_value(node.args[0]) and is_simple_assignment_value(node.args[1])
    return False


def source_top_level_assignments(source: str) -> dict[str, str]:
    tree = ast.parse(source)
    result: dict[str, str] = {}
    for stmt in tree.body:
        if not isinstance(stmt, ast.Assign) or len(stmt.targets) != 1:
            continue
        target = stmt.targets[0]
        if not isinstance(target, ast.Name):
            continue
        if has_private_name(target.id) or target.id == "__all__":
            continue
        if is_simple_assignment_value(stmt.value):
            result[target.id] = f"{target.id} = {ast.unparse(stmt.value)}"
    return result


def restore_top_level_assignment_values(source_stub: str, source_runtime: str) -> str:
    assignments = source_top_level_assignments(source_runtime)
    if not assignments:
        return source_stub

    tree = ast.parse(source_stub)
    lines = source_stub.splitlines()
    restored_names: set[str] = set()
    for stmt in tree.body:
        if (
            isinstance(stmt, ast.AnnAssign)
            and stmt.value is None
            and isinstance(stmt.target, ast.Name)
            and stmt.target.id in assignments
        ):
            name = stmt.target.id
            lines[stmt.lineno - 1] = assignments[name]
            for index in range(stmt.lineno, stmt.end_lineno or stmt.lineno):
                lines[index] = ""
            restored_names.add(name)

    if not restored_names:
        return source_stub

    restored = "\n".join(lines).rstrip() + "\n"
    typing_names = {
        name
        for name in ("Literal", "NewType", "cast")
        if any(name_used_in(name, assignments[restored_name]) for restored_name in restored_names)
    }
    if typing_names:
        restored = "\n".join(ensure_typing_import(restored.splitlines(), typing_names)).rstrip() + "\n"
    return restored


def source_class_assignments(source: str) -> dict[str, dict[str, str]]:
    tree = ast.parse(source)
    result: dict[str, dict[str, str]] = {}
    for stmt in tree.body:
        if not isinstance(stmt, ast.ClassDef):
            continue
        assignments: dict[str, str] = {}
        for child in stmt.body:
            if isinstance(child, ast.Assign):
                rendered = simple_class_assignment(child)
                if rendered:
                    assignments[rendered.split("=", 1)[0].strip()] = rendered
        if assignments:
            result[stmt.name] = assignments
    return result


def restore_class_assignment_values(source_stub: str, source_runtime: str) -> str:
    assignments = source_class_assignments(source_runtime)
    if not assignments:
        return source_stub

    lines = source_stub.splitlines()
    current_class: str | None = None
    seen: dict[str, set[str]] = {class_name: set() for class_name in assignments}
    output: list[str] = []

    for line in lines:
        class_match = re.match(r"^class\s+([A-Za-z_][A-Za-z0-9_]*)\b", line)
        if class_match:
            current_class = class_match.group(1)
            output.append(line)
            continue
        if line and not line.startswith((" ", "\t")):
            if current_class in assignments:
                for name, rendered in assignments[current_class].items():
                    if name not in seen[current_class]:
                        output.append(f"    {rendered}")
            current_class = None

        if current_class in assignments:
            assign_match = re.match(r"^    ([A-Za-z_][A-Za-z0-9_]*)(?::| =)", line)
            if assign_match and assign_match.group(1) in assignments[current_class]:
                name = assign_match.group(1)
                output.append(f"    {assignments[current_class][name]}")
                seen[current_class].add(name)
                continue
        output.append(line)

    if current_class in assignments:
        for name, rendered in assignments[current_class].items():
            if name not in seen[current_class]:
                output.append(f"    {rendered}")

    restored = "\n".join(output).rstrip() + "\n"
    if "cast(" in restored:
        restored = "\n".join(ensure_typing_import(restored.splitlines(), {"cast"})).rstrip() + "\n"
    return restored


def read_exports(source_stub: str, source_runtime: str, only_names: frozenset[str] | None) -> tuple[str, ...]:
    if only_names:
        return tuple(only_names)
    try:
        source_tree = ast.parse(source_runtime)
        source_all = get_source_all(source_tree)
        stub_names = set(public_names_from_tree(ast.parse(source_stub)))
        if source_all:
            return tuple(name for name in source_all if name in stub_names)
        return public_names_from_tree(ast.parse(source_stub))
    except SyntaxError:
        return ()


def add_module_all(source: str, exports: tuple[str, ...]) -> str:
    source = remove_module_all(source)
    if not exports:
        return source
    all_line = "__all__ = [" + ", ".join(f'"{name}"' for name in exports) + "]"
    lines = source.splitlines()
    insert_at = 0
    while insert_at < len(lines) and (
        lines[insert_at].startswith("#")
        or lines[insert_at] == ""
        or lines[insert_at].startswith("from __future__")
        or lines[insert_at].startswith("import ")
        or lines[insert_at].startswith("from ")
        or lines[insert_at] in {"try:", "except ImportError:"}
        or lines[insert_at].startswith("    ")
    ):
        insert_at += 1
    lines.insert(insert_at, "")
    lines.insert(insert_at, all_line)
    return "\n".join(lines).rstrip() + "\n"


def remove_module_all(source: str) -> str:
    tree = ast.parse(source)
    lines = source.splitlines()
    for stmt in tree.body:
        if not isinstance(stmt, ast.Assign):
            continue
        if not any(isinstance(target, ast.Name) and target.id == "__all__" for target in stmt.targets):
            continue
        for index in range(stmt.lineno - 1, stmt.end_lineno or stmt.lineno):
            lines[index] = ""
    return "\n".join(lines).rstrip() + "\n"


def ensure_integration_registry(source: str, rel_path: Path) -> str:
    if rel_path != Path("integrations/integration.py") or name_used_in("_registry", source):
        return source

    lines = source.splitlines()
    insert_at = 0
    while insert_at < len(lines) and (
        lines[insert_at].startswith("#")
        or lines[insert_at] == ""
        or lines[insert_at].startswith("from __future__")
        or lines[insert_at].startswith("import ")
        or lines[insert_at].startswith("from ")
        or lines[insert_at].startswith("if TYPE_CHECKING")
        or lines[insert_at].startswith("    ")
    ):
        insert_at += 1
    lines.insert(insert_at, "")
    lines.insert(insert_at, "_registry: dict[str, type[Integration]] = {}")
    return "\n".join(lines).rstrip() + "\n"


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


def postprocess_stub(stub_source: str, runtime_source: str, spec: SourceSpec) -> tuple[str, tuple[str, ...]]:
    source = replace_incomplete_with_any(stub_source)
    source = rewrite_imports(source, spec.rel_path)
    source = move_named_imports_to_type_checking(source, source_type_checking_bound_names(runtime_source))
    source = replace_unresolvable_types(source)
    source = restore_top_level_assignment_values(source, runtime_source)
    source = restore_class_assignment_values(source, runtime_source)
    source = filter_top_level_names(source, spec.filter_names)
    source = prune_type_checking_imports(source)
    source = prune_unused_imports(source)
    source = apply_pydantic_fallback(source)
    source = ensure_integration_registry(source, spec.rel_path)
    source = collapse_blank_lines(source)
    available_names = available_bound_names(source)
    exports = tuple(name for name in spec.exports if name in available_names)
    source = add_module_all(source, exports)
    return STUB_HEADER.rstrip() + "\n\n" + source.lstrip(), exports


def read_all_from_generated_stub(path: Path) -> tuple[str, ...]:
    try:
        tree = ast.parse(path.read_text(encoding="utf-8"))
    except SyntaxError:
        return ()
    all_names = get_source_all(tree)
    return tuple(all_names or ())


def generated_module_name(path: Path) -> str:
    return ".".join(path.with_suffix("").parts)


def generate_root_init(
    modules: list[GeneratedModule],
    decorator_exports: tuple[str, ...],
    secret_vault_exports: tuple[str, ...],
) -> str:
    export_entries: list[str] = []
    import_blocks: list[str] = []

    for module in sorted(modules, key=lambda item: item.module_name):
        if module.path.name == "__init__.py" or not module.exports:
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

    if secret_vault_exports:
        export_entries.extend(secret_vault_exports)
        import_blocks.append(
            "from _gen.secret_vault import (\n    "
            + ", ".join(secret_vault_exports)
            + "\n)"
        )

    all_block = "__all__ = [\n"
    if export_entries:
        all_block += "    " + ",\n    ".join(f'"{name}"' for name in export_entries) + "\n"
    all_block += "]\n\n"

    return INIT_HEADER + all_block + "\n".join(import_blocks) + ("\n" if import_blocks else "")


def ensure_package_inits(output_dir: Path) -> None:
    for directory in sorted(path for path in output_dir.rglob("*") if path.is_dir()):
        init_path = directory / "__init__.py"
        if not init_path.exists():
            init_path.write_text(STUB_HEADER + "\n__all__ = []\n", encoding="utf-8")


def copy_local_overlays(template_dir: Path, generated_dir: Path) -> tuple[tuple[str, ...], tuple[str, ...]]:
    decorator_exports: tuple[str, ...] = ()
    secret_vault_exports: tuple[str, ...] = ()
    for rel in LOCAL_OVERLAY_FILES:
        source = template_dir / rel
        if not source.exists():
            continue
        dest = generated_dir / rel
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, dest)
        if rel == "decorators.py":
            decorator_exports = read_all_from_generated_stub(dest)
        elif rel == "secret_vault.py":
            secret_vault_exports = read_all_from_generated_stub(dest)
    return decorator_exports, secret_vault_exports


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
            stub_source = stub_path.read_text(encoding="utf-8")
            runtime_source = spec.source_path.read_text(encoding="utf-8")
            generated_source, exports = postprocess_stub(stub_source, runtime_source, spec)

            dest = output_dir / spec.rel_path
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_text(generated_source, encoding="utf-8")
            modules.append(
                GeneratedModule(
                    path=spec.rel_path,
                    module_name=generated_module_name(spec.rel_path),
                    exports=exports,
                )
            )

    ensure_package_inits(output_dir)
    decorator_exports, secret_vault_exports = copy_local_overlays(template_dir, output_dir)
    root_init = generate_root_init(modules, decorator_exports, secret_vault_exports)
    (output_dir / "__init__.py").write_text(root_init, encoding="utf-8")


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
