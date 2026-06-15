#!/usr/bin/env python3
"""Generate Rust ADK _gen templates from genai_lambda_runtime sources.

This is intentionally owned by the Rust ADK repo. It borrows the broad shape of
the Python ADK stub sync workflow, but writes the final vendored template tree
used by `poly init` and `poly pull`:

    adk-core/python-gen-template/

The runtime package is the canonical input. The Python ADK repository is not an
input.
"""

from __future__ import annotations

import argparse
import ast
import filecmp
import re
import shutil
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
        "utils.api_connector",
        "utils.secret_vault",
    }
)

ALLOWED_IMPORT_MODULES = frozenset(
    {
        "abc",
        "collections",
        "collections.abc",
        "dataclasses",
        "datetime",
        "enum",
        "pydantic",
        "re",
        "typing",
    }
)

TYPE_ONLY_IMPORT_MODULES = frozenset({"requests"})


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


def destination_for_module(module_name: str) -> Path:
    return Path(*module_name.split(".")).with_suffix(".py")


def current_package_for_module(module_name: str) -> tuple[str, ...]:
    parts = tuple(module_name.split("."))
    if parts[-1] == "__init__":
        return parts[:-1]
    return parts[:-1]


def should_generate_module(runtime_path: Path, source_path: Path) -> bool:
    if source_path.name.startswith("_") and source_path.name != "__init__.py":
        return False
    module_name = module_name_for_path(runtime_path, source_path)
    if module_name == "__init__":
        return False
    return module_name not in SKIPPED_MODULES


def format_annotation(node: ast.expr | None) -> str | None:
    if node is None:
        return None
    return ast.unparse(node)


def format_arg(arg: ast.arg) -> str:
    annotation = format_annotation(arg.annotation)
    if annotation:
        return f"{arg.arg}: {annotation}"
    return arg.arg


def build_signature(func: ast.FunctionDef | ast.AsyncFunctionDef) -> str:
    args = func.args
    parts: list[str] = []

    for arg in args.posonlyargs:
        parts.append(format_arg(arg))
    if args.posonlyargs:
        parts.append("/")

    num_defaults = len(args.defaults)
    num_args = len(args.args)
    for index, arg in enumerate(args.args):
        formatted = format_arg(arg)
        if index >= num_args - num_defaults:
            formatted += " = ..."
        parts.append(formatted)

    if args.vararg:
        parts.append(f"*{format_arg(args.vararg)}")
    elif args.kwonlyargs:
        parts.append("*")

    for index, arg in enumerate(args.kwonlyargs):
        formatted = format_arg(arg)
        if args.kw_defaults[index] is not None:
            formatted += " = ..."
        parts.append(formatted)

    if args.kwarg:
        parts.append(f"**{format_arg(args.kwarg)}")

    signature = ", ".join(parts)
    return_annotation = format_annotation(func.returns)
    if return_annotation:
        return f"({signature}) -> {return_annotation}"
    return f"({signature})"


def get_docstring(node: ast.AST) -> str | None:
    if not isinstance(node, (ast.Module, ast.ClassDef, ast.FunctionDef, ast.AsyncFunctionDef)):
        return None
    return ast.get_docstring(node, clean=True)


def first_docstring_line(node: ast.AST) -> str | None:
    docstring = get_docstring(node)
    if not docstring:
        return None
    return docstring.strip().split("\n")[0]


def is_property(func: ast.FunctionDef | ast.AsyncFunctionDef) -> bool:
    return any(
        (isinstance(decorator, ast.Name) and decorator.id == "property")
        or (isinstance(decorator, ast.Attribute) and decorator.attr == "property")
        for decorator in func.decorator_list
    )


def has_named_decorator(func: ast.FunctionDef | ast.AsyncFunctionDef, name: str) -> bool:
    return any(
        isinstance(decorator, ast.Name) and decorator.id == name
        for decorator in func.decorator_list
    )


def extract_method_stub(func: ast.FunctionDef | ast.AsyncFunctionDef) -> str | None:
    if has_private_name(func.name):
        return None

    lines: list[str] = []
    if is_property(func):
        lines.append("    @property")
    elif has_named_decorator(func, "staticmethod"):
        lines.append("    @staticmethod")
    elif has_named_decorator(func, "classmethod"):
        lines.append("    @classmethod")

    prefix = "async def" if isinstance(func, ast.AsyncFunctionDef) else "def"
    docstring = first_docstring_line(func)
    if docstring:
        lines.append(f"    {prefix} {func.name}{build_signature(func)}:")
        lines.append(f'        """{docstring}"""')
        lines.append("        ...")
    else:
        lines.append(f"    {prefix} {func.name}{build_signature(func)}: ...")
    return "\n".join(lines)


def is_dataclass(cls: ast.ClassDef) -> bool:
    for decorator in cls.decorator_list:
        if isinstance(decorator, ast.Name) and decorator.id == "dataclass":
            return True
        if isinstance(decorator, ast.Attribute) and decorator.attr == "dataclass":
            return True
        if isinstance(decorator, ast.Call):
            if isinstance(decorator.func, ast.Name) and decorator.func.id == "dataclass":
                return True
            if isinstance(decorator.func, ast.Attribute) and decorator.func.attr == "dataclass":
                return True
    return False


def synthesize_dataclass_init(cls: ast.ClassDef) -> str | None:
    params = ["self"]
    for stmt in cls.body:
        if not isinstance(stmt, ast.AnnAssign) or not isinstance(stmt.target, ast.Name):
            continue
        if has_private_name(stmt.target.id):
            continue
        annotation = format_annotation(stmt.annotation) or "Any"
        default = " = ..." if stmt.value is not None else ""
        params.append(f"{stmt.target.id}: {annotation}{default}")
    if len(params) == 1:
        return None
    return f"    def __init__({', '.join(params)}) -> None: ..."


def extract_class_vars(cls: ast.ClassDef) -> list[str]:
    lines: list[str] = []
    for stmt in cls.body:
        if isinstance(stmt, ast.AnnAssign) and isinstance(stmt.target, ast.Name):
            if has_private_name(stmt.target.id):
                continue
            annotation = format_annotation(stmt.annotation) or "Any"
            lines.append(f"    {stmt.target.id}: {annotation}")
    return lines


def extract_class_stub(cls: ast.ClassDef) -> str | None:
    if has_private_name(cls.name):
        return None

    bases = ", ".join(ast.unparse(base) for base in cls.bases)
    base_clause = f"({bases})" if bases else ""
    lines = [f"class {cls.name}{base_clause}:"]

    docstring = first_docstring_line(cls)
    if docstring:
        lines.append(f'    """{docstring}"""')

    body: list[str] = []
    body.extend(extract_class_vars(cls))

    has_explicit_init = any(
        isinstance(stmt, ast.FunctionDef) and stmt.name == "__init__" for stmt in cls.body
    )
    if is_dataclass(cls) and not has_explicit_init:
        init = synthesize_dataclass_init(cls)
        if init:
            body.append(init)

    for stmt in cls.body:
        if isinstance(stmt, (ast.FunctionDef, ast.AsyncFunctionDef)):
            method = extract_method_stub(stmt)
            if method:
                body.append(method)

    if not body and not docstring:
        lines.append("    ...")
    else:
        if docstring and body:
            lines.append("")
        lines.extend(body)

    return "\n".join(lines)


def extract_function_stub(func: ast.FunctionDef | ast.AsyncFunctionDef) -> str | None:
    if has_private_name(func.name):
        return None
    prefix = "async def" if isinstance(func, ast.AsyncFunctionDef) else "def"
    docstring = first_docstring_line(func)
    if docstring:
        return "\n".join(
            [
                f"{prefix} {func.name}{build_signature(func)}:",
                f'    """{docstring}"""',
                "    ...",
            ]
        )
    return f"{prefix} {func.name}{build_signature(func)}: ..."


def assignment_name(stmt: ast.Assign | ast.AnnAssign) -> str | None:
    if isinstance(stmt, ast.Assign):
        if len(stmt.targets) != 1 or not isinstance(stmt.targets[0], ast.Name):
            return None
        return stmt.targets[0].id
    if isinstance(stmt.target, ast.Name):
        return stmt.target.id
    return None


def extract_assignment(stmt: ast.Assign | ast.AnnAssign) -> tuple[str, str] | None:
    name = assignment_name(stmt)
    if not name or has_private_name(name):
        return None
    return name, ast.unparse(stmt)


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


def collect_import_statements(stmts: list[ast.stmt]) -> list[ast.stmt]:
    result: list[ast.stmt] = []
    for stmt in stmts:
        if isinstance(stmt, (ast.Import, ast.ImportFrom)):
            result.append(stmt)
        elif (
            isinstance(stmt, ast.If)
            and isinstance(stmt.test, ast.Name)
            and stmt.test.id == "TYPE_CHECKING"
        ):
            result.extend(collect_import_statements(stmt.body))
    return result


def is_allowed_import_module(module: str) -> bool:
    root = module.split(".", 1)[0]
    return module in ALLOWED_IMPORT_MODULES or root in ALLOWED_IMPORT_MODULES


def is_type_only_import_module(module: str) -> bool:
    root = module.split(".", 1)[0]
    return module in TYPE_ONLY_IMPORT_MODULES or root in TYPE_ONLY_IMPORT_MODULES


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


def import_from_line(module: str, names: list[ast.alias], current_package: tuple[str, ...]) -> str | None:
    if not names:
        return None
    runtime_module = runtime_relative_module(module, current_package)
    if runtime_module is not None:
        module = runtime_module
    elif not is_allowed_import_module(module) and not is_type_only_import_module(module):
        return None

    rendered_names = []
    for alias in names:
        if has_private_name(alias.name):
            continue
        if alias.asname:
            rendered_names.append(f"{alias.name} as {alias.asname}")
        else:
            rendered_names.append(alias.name)
    if not rendered_names:
        return None
    return f"from {module} import {', '.join(rendered_names)}"


def collect_imports(tree: ast.Module, current_package: tuple[str, ...]) -> list[str | tuple[str, str, str]]:
    imports: list[str | tuple[str, str, str]] = []
    for stmt in collect_import_statements(tree.body):
        if isinstance(stmt, ast.Import):
            for alias in stmt.names:
                if alias.name.startswith(("runtime.", "utils.")):
                    bound_name = alias.asname or alias.name.rsplit(".", 1)[-1]
                    imports.append(("module_alias", alias.name, bound_name))
                elif is_allowed_import_module(alias.name) or is_type_only_import_module(alias.name):
                    imports.append(ast.unparse(ast.Import(names=[alias])))
        elif isinstance(stmt, ast.ImportFrom):
            if not stmt.module:
                continue
            line = import_from_line(stmt.module, stmt.names, current_package)
            if line:
                imports.append(line)
    return imports


def name_used_in(name: str, body_text: str) -> bool:
    return bool(re.search(rf"(?<![A-Za-z0-9_]){re.escape(name)}(?![A-Za-z0-9_])", body_text))


def expand_module_alias(
    module: str,
    alias: str,
    body_text: str,
    current_package: tuple[str, ...],
) -> tuple[str | None, str]:
    pattern = re.compile(rf"(?<![A-Za-z0-9_.]){re.escape(alias)}\.(\w+)")
    names = sorted({match.group(1) for match in pattern.finditer(body_text)})
    if not names:
        return None, body_text

    runtime_module = runtime_relative_module(module, current_package)
    import_module = runtime_module or module
    rewritten = pattern.sub(r"\1", body_text)
    return f"from {import_module} import {', '.join(names)}", rewritten


def prune_imports(import_lines: list[str], body_text: str) -> list[str]:
    pruned: list[str] = []
    type_checking: list[str] = []
    seen: set[str] = set()
    for line in import_lines:
        try:
            node = ast.parse(line).body[0]
        except SyntaxError:
            continue

        if isinstance(node, ast.Import):
            kept_aliases = []
            kept_type_only_aliases = []
            for alias in node.names:
                bound_name = alias.asname or alias.name.split(".", 1)[0]
                if name_used_in(bound_name, body_text):
                    if is_type_only_import_module(alias.name):
                        kept_type_only_aliases.append(alias)
                    else:
                        kept_aliases.append(alias)
            if kept_type_only_aliases:
                type_checking.append(ast.unparse(ast.Import(names=kept_type_only_aliases)))
            if not kept_aliases:
                continue
            rendered = ast.unparse(ast.Import(names=kept_aliases))
        elif isinstance(node, ast.ImportFrom):
            kept_aliases = [
                alias
                for alias in node.names
                if name_used_in(alias.asname or alias.name, body_text)
            ]
            if not kept_aliases:
                continue
            if node.module and is_type_only_import_module(node.module):
                type_checking.append(
                    ast.unparse(ast.ImportFrom(module=node.module, names=kept_aliases, level=node.level))
                )
                continue
            elif node.module == "pydantic" and any(alias.name == "BaseModel" for alias in kept_aliases):
                rendered = "\n".join(
                    [
                        "try:",
                        "    from pydantic import BaseModel",
                        "except ImportError:",
                        "    class BaseModel:",
                        "        ...",
                    ]
                )
            else:
                rendered = ast.unparse(
                    ast.ImportFrom(module=node.module, names=kept_aliases, level=node.level)
                )
        else:
            continue

        if rendered not in seen:
            seen.add(rendered)
            pruned.append(rendered)
    if type_checking:
        rendered = "from typing import TYPE_CHECKING\n\nif TYPE_CHECKING:\n" + "\n".join(
            f"    {line}" for line in dict.fromkeys(type_checking)
        )
        if rendered not in seen:
            pruned.append(rendered)
    return pruned


def generate_stub(
    source_path: Path,
    module_name: str,
    *,
    only_names: set[str] | None = None,
    include_imports: bool = True,
) -> tuple[str, tuple[str, ...]]:
    source = source_path.read_text(encoding="utf-8")
    tree = ast.parse(source)
    current_package = current_package_for_module(module_name)

    assignments_before: list[str] = []
    assignments_after: list[str] = []
    classes: list[str] = []
    functions: list[str] = []
    public_names: list[str] = []

    class_names = {stmt.name for stmt in tree.body if isinstance(stmt, ast.ClassDef)}

    for stmt in tree.body:
        if isinstance(stmt, (ast.Assign, ast.AnnAssign)):
            assignment = extract_assignment(stmt)
            if not assignment:
                continue
            name, line = assignment
            if only_names is not None and name not in only_names:
                continue
            if name == "__all__":
                continue
            public_names.append(name)
            value_text = ast.unparse(stmt.value) if getattr(stmt, "value", None) is not None else ""
            if any(class_name in value_text for class_name in class_names):
                assignments_after.append(line)
            else:
                assignments_before.append(line)
        elif isinstance(stmt, ast.ClassDef):
            if only_names is not None and stmt.name not in only_names:
                continue
            class_stub = extract_class_stub(stmt)
            if class_stub:
                public_names.append(stmt.name)
                classes.append(class_stub)
        elif isinstance(stmt, (ast.FunctionDef, ast.AsyncFunctionDef)):
            if only_names is not None and stmt.name not in only_names:
                continue
            function_stub = extract_function_stub(stmt)
            if function_stub:
                public_names.append(stmt.name)
                functions.append(function_stub)

    source_all = get_source_all(tree)
    exports = tuple(name for name in (source_all or public_names) if name in public_names)

    body_parts: list[str] = []
    if assignments_before:
        body_parts.append("\n".join(assignments_before))
    if classes:
        body_parts.append("\n\n".join(classes))
    if functions:
        body_parts.append("\n\n".join(functions))
    if assignments_after:
        body_parts.append("\n".join(assignments_after))
    body_text = "\n\n".join(body_parts)

    imports: list[str] = []
    if include_imports:
        resolved_imports: list[str] = []
        for item in collect_imports(tree, current_package):
            if isinstance(item, tuple) and item[0] == "module_alias":
                _, module, alias = item
                import_line, body_text = expand_module_alias(
                    module, alias, body_text, current_package
                )
                if import_line:
                    resolved_imports.append(import_line)
            else:
                resolved_imports.append(item)

        imports = prune_imports(resolved_imports, body_text)

    parts = [STUB_HEADER.rstrip()]
    if imports:
        parts.append("\n".join(imports))
    if exports:
        joined_exports = ", ".join(f'"{name}"' for name in exports)
        parts.append(f"__all__ = [{joined_exports}]")
    if body_text.strip():
        parts.append(body_text)

    return "\n\n".join(parts) + "\n", exports


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

    modules: list[GeneratedModule] = []
    for source_path in sorted(runtime_path.rglob("*.py")):
        if "__pycache__" in source_path.parts:
            continue
        if not should_generate_module(runtime_path, source_path):
            continue
        module_name = module_name_for_path(runtime_path, source_path)
        rel_path = destination_for_module(module_name)
        stub, exports = generate_stub(source_path, module_name)
        dest = output_dir / rel_path
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_text(stub, encoding="utf-8")
        modules.append(GeneratedModule(path=rel_path, module_name=generated_module_name(rel_path), exports=exports))

    api_connector_source = runtime_path.parent / "utils" / "api_connector.py"
    if api_connector_source.exists():
        rel_path = Path("api_connector.py")
        stub, exports = generate_stub(
            api_connector_source,
            "api_connector",
            only_names={"ApiIntegrations"},
            include_imports=False,
        )
        dest = output_dir / rel_path
        dest.write_text(stub, encoding="utf-8")
        modules.append(
            GeneratedModule(
                path=rel_path,
                module_name=generated_module_name(rel_path),
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
