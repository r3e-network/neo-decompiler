#!/usr/bin/env python3
"""
Generate Rust lookup tables for Neo N3 native contracts and their exposed method names.
"""

import base64
import hashlib
import json
import re
import urllib.error
import urllib.request
from dataclasses import dataclass, field
from pathlib import Path
from typing import Dict, Iterable, List, Set

REPO_ROOT = Path(__file__).resolve().parents[1]
LOCAL_NATIVE_DIR = REPO_ROOT / "neo_csharp" / "core" / "src" / "Neo" / "SmartContract" / "Native"
API_BASE_URL = "https://api.github.com/repos/neo-project/neo/contents/src/Neo/SmartContract/Native/"
OUTPUT = REPO_ROOT / "src" / "native_contracts_generated.rs"


@dataclass
class NativeContract:
    class_name: str
    name: str
    script_hash: bytes
    methods: list[str] = field(default_factory=list)


@dataclass
class ClassInfo:
    name: str
    bases: list[str]
    methods: list[str]


def read_local(path: str) -> str | None:
    local_path = LOCAL_NATIVE_DIR / path
    if local_path.exists():
        return local_path.read_text(encoding="utf-8")
    return None


def fetch(path: str) -> str:
    url = f"{API_BASE_URL}{path}?ref=master"
    headers = {
        "User-Agent": "neo-decompiler/0.1",
        "Accept": "application/vnd.github+json",
    }
    for attempt in range(3):
        try:
            req = urllib.request.Request(url, headers=headers)
            with urllib.request.urlopen(req) as resp:  # type: ignore[arg-type]
                payload = json.loads(resp.read().decode("utf-8"))
            if payload.get("encoding") != "base64":
                raise ValueError(f"unexpected encoding for {path}")
            return base64.b64decode(payload["content"]).decode("utf-8")
        except urllib.error.URLError:  # type: ignore[attr-defined]
            if attempt == 2:
                raise
    raise RuntimeError(f"unreachable: failed to fetch {path}")


_REMOTE_DIR_CACHE: list[str] | None = None


def list_remote_files() -> list[str]:
    global _REMOTE_DIR_CACHE
    if _REMOTE_DIR_CACHE is not None:
        return _REMOTE_DIR_CACHE
    url = f"{API_BASE_URL}?ref=master"
    headers = {
        "User-Agent": "neo-decompiler/0.1",
        "Accept": "application/vnd.github+json",
    }
    req = urllib.request.Request(url, headers=headers)
    with urllib.request.urlopen(req) as resp:  # type: ignore[arg-type]
        payload = json.loads(resp.read().decode("utf-8"))
    _REMOTE_DIR_CACHE = [
        entry["name"]
        for entry in payload
        if entry.get("type") == "file" and isinstance(entry.get("name"), str)
    ]
    return _REMOTE_DIR_CACHE


def load_source(path: str) -> str:
    local = read_local(path)
    if local is not None:
        return local
    return fetch(path)


CONTRACT_PROPERTY_PATTERN = re.compile(
    r"public\s+static\s+(?P<class>[A-Za-z0-9_]+)\s+(?P<name>[A-Za-z0-9_]+)\s*{\s*get;\s*}\s*=\s*new\(\);"
)


def parse_contract_class_names(native_contract_source: str) -> list[str]:
    classes = []
    for match in CONTRACT_PROPERTY_PATTERN.finditer(native_contract_source):
        classes.append(match.group("class"))
    return classes


CLASS_DECL_PATTERN = re.compile(
    r"class\s+(?P<name>[A-Za-z0-9_]+)\s*:\s*(?P<bases>[A-Za-z0-9_,\s]+)")

METHOD_SIGNATURE_PATTERN = re.compile(
    r"(?:public|internal|protected|private)\s+(?:async\s+)?(?:static\s+)?"
    r"(?:[\w<>\[\],\s\.\?]+)\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)\s*\(",
    re.MULTILINE,
)
NAME_OVERRIDE_PATTERN = re.compile(r'Name\s*=\s*"(?P<name>[^"]+)"')


def extract_contract_methods(source: str) -> list[str]:
    methods: list[str] = []
    pending = False
    buffer: list[str] = []
    name_override: str | None = None

    for line in source.splitlines():
        stripped = line.strip()
        if stripped.startswith("[ContractMethod"):
            pending = True
            buffer.clear()
            match = NAME_OVERRIDE_PATTERN.search(stripped)
            name_override = match.group("name") if match else None
            continue
        if not pending:
            continue
        if stripped.startswith("["):
            # additional attribute, keep waiting
            continue
        buffer.append(stripped)
        joined = " ".join(buffer)
        match = METHOD_SIGNATURE_PATTERN.search(joined)
        if match:
            name = name_override or match.group("name")
            if name not in methods:
                methods.append(name)
            pending = False
            buffer.clear()
            name_override = None
    return methods


def parse_class_info(sources: Iterable[str]) -> ClassInfo:
    bases: List[str] = []
    methods: list[str] = []
    name = "Unknown"
    for source in sources:
        match = CLASS_DECL_PATTERN.search(source)
        if match:
            name = match.group("name")
            bases_text = match.group("bases")
            bases.extend(part.strip() for part in bases_text.split(","))
        methods.extend(extract_contract_methods(source))
    bases = sorted(set(bases))
    methods = sorted(set(methods))
    return ClassInfo(name=name, bases=bases, methods=methods)


def contract_hash(name: str) -> bytes:
    OP_ABORT = 0x38
    PUSHDATA1 = 0x0C
    PUSH0 = 0x10

    def emit_push_data(data: bytes) -> bytes:
        if len(data) < 0x100:
            return bytes([PUSHDATA1, len(data)]) + data
        if len(data) < 0x10000:
            return bytes([0x0D]) + len(data).to_bytes(2, "little") + data
        return bytes([0x0E]) + len(data).to_bytes(4, "little") + data

    script = bytearray()
    script.append(OP_ABORT)
    script.extend(emit_push_data(bytes(20)))
    script.append(PUSH0)
    script.extend(emit_push_data(name.encode("utf-8")))

    sha = hashlib.sha256(bytes(script)).digest()
    return hashlib.new("ripemd160", sha).digest()


def collect_contracts() -> list[NativeContract]:
    native_contract_src = load_source("NativeContract.cs")
    class_names = parse_contract_class_names(native_contract_src)

    class_info: Dict[str, ClassInfo] = {}

    def ensure_class_loaded(name: str) -> None:
        if name in class_info or name == "NativeContract":
            return
        sources: list[str] = []
        local_sources = list(LOCAL_NATIVE_DIR.glob(f"{name}*.cs")) if LOCAL_NATIVE_DIR.exists() else []
        if local_sources:
            sources = [path.read_text(encoding="utf-8") for path in local_sources]
        else:
            remote_files = [
                filename
                for filename in list_remote_files()
                if filename.startswith(f"{name}.") or filename == f"{name}.cs"
            ]
            if not remote_files:
                class_info[name] = ClassInfo(name=name, bases=[], methods=[])
                return
            for filename in remote_files:
                try:
                    sources.append(fetch(filename))
                except urllib.error.HTTPError:  # type: ignore[attr-defined]
                    continue
        if not sources:
            class_info[name] = ClassInfo(name=name, bases=[], methods=[])
            return
        info = parse_class_info(sources)
        class_info[name] = info
        for base in info.bases:
            ensure_class_loaded(base)

    for class_name in class_names:
        ensure_class_loaded(class_name)

    def collect_methods_recursive(name: str, seen: Set[str]) -> List[str]:
        if name in seen:
            return []
        seen.add(name)
        info = class_info.get(name)
        if not info:
            return []
        methods = list(info.methods)
        for base in info.bases:
            methods.extend(collect_methods_recursive(base, seen))
        return methods

    contracts: list[NativeContract] = []
    for class_name in class_names:
        methods = collect_methods_recursive(class_name, set())
        script_hash = contract_hash(class_name)
        contracts.append(
            NativeContract(
                class_name=class_name,
                name=class_name,
                script_hash=script_hash,
                methods=sorted(set(methods)),
            )
        )
    contracts.sort(key=lambda c: c.script_hash)
    return contracts


RUST_TEMPLATE = """// This file is @generated by tools/scrape_native_contracts.py. Do not edit manually.

pub struct NativeContractInfo {{
    pub name: &'static str,
    pub script_hash: [u8; 20],
    pub methods: &'static [&'static str],
}}

pub const NATIVE_CONTRACTS: &[NativeContractInfo] = &[
{entries}
];
"""


def render_contract(contract: NativeContract) -> str:
    hash_bytes = ", ".join(f"0x{b:02X}" for b in contract.script_hash)
    methods = ", ".join(f"\"{m}\"" for m in contract.methods)
    return (
        "    NativeContractInfo {\n"
        f"        name: \"{contract.name}\",\n"
        f"        script_hash: [{hash_bytes}],\n"
        f"        methods: &[{methods}],\n"
        "    },"
    )


def main() -> None:
    contracts = collect_contracts()
    entries = "\n".join(render_contract(c) for c in contracts)
    OUTPUT.write_text(RUST_TEMPLATE.format(entries=entries))
    data_dir = REPO_ROOT / "tools" / "data"
    data_dir.mkdir(parents=True, exist_ok=True)
    meta = [
        {
            "class_name": c.class_name,
            "name": c.name,
            "script_hash": c.script_hash.hex(),
            "methods": c.methods,
        }
        for c in contracts
    ]
    (data_dir / "native_contracts.json").write_text(json.dumps(meta, indent=2))
    print(f"wrote {OUTPUT} ({len(contracts)} contracts)")


if __name__ == "__main__":
    main()
