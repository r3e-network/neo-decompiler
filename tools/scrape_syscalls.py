#!/usr/bin/env python3
"""
Generate Rust lookup tables for Neo syscalls by scraping ApplicationEngine partial classes.
"""

import hashlib
import json
import re
import urllib.error
import urllib.request
from collections.abc import Iterable
from dataclasses import dataclass
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
LOCAL_BASE = REPO_ROOT / "neo_csharp" / "core" / "src" / "Neo" / "SmartContract"
RAW_BASE_URL = "https://raw.githubusercontent.com/neo-project/neo/master/src/Neo/SmartContract/"
FILES = [
    "ApplicationEngine.Runtime.cs",
    "ApplicationEngine.Contract.cs",
    "ApplicationEngine.Crypto.cs",
    "ApplicationEngine.Storage.cs",
    "ApplicationEngine.Iterator.cs",
]

OUTPUT = REPO_ROOT / "src" / "syscalls_generated.rs"


@dataclass
class Syscall:
    name: str
    handler: str
    price: str
    call_flags: str
    returns_value: bool
    param_count: int

    @property
    def hash_value(self) -> int:
        digest = hashlib.sha256(self.name.encode("ascii")).digest()
        return int.from_bytes(digest[:4], "little")

    def as_dict(self) -> dict:
        return {
            "name": self.name,
            "handler": self.handler,
            "price": self.price,
            "call_flags": self.call_flags,
            "hash": self.hash_value,
            "returns_value": self.returns_value,
            "param_count": self.param_count,
        }


REGISTER_PATTERN = re.compile(
    r'Register\("(?P<name>[^"]+)",\s*(?:nameof\()?(?P<handler>[A-Za-z0-9_\.]+)\)?\s*,\s*(?P<price>[^,]+),\s*(?P<flags>CallFlags\.[A-Za-z0-9_\s|&^\.]+)(?:,\s*Hardfork\.[A-Za-z0-9_]+)?\)',
    re.MULTILINE,
)


def read_local(path: str) -> str | None:
    local_path = LOCAL_BASE / path
    if local_path.exists():
        return local_path.read_text(encoding="utf-8")
    return None


def fetch(path: str) -> str:
    url = f"{RAW_BASE_URL}{path}"
    headers = {
        "User-Agent": "neo-decompiler/0.1",
        "Accept": "text/plain",
    }
    for attempt in range(3):
        try:
            req = urllib.request.Request(url, headers=headers)
            with urllib.request.urlopen(req) as resp:  # type: ignore[arg-type]
                return resp.read().decode("utf-8")
        except urllib.error.URLError:  # type: ignore[attr-defined]
            if attempt == 2:
                raise
    raise RuntimeError(f"unreachable: failed to fetch {path}")


def try_fetch(path: str) -> str | None:
    try:
        return fetch(path)
    except urllib.error.URLError:  # type: ignore[attr-defined]
        return None
    except urllib.error.HTTPError:  # type: ignore[attr-defined]
        return None


def dedupe_preserve_order(items: Iterable[str]) -> list[str]:
    out: list[str] = []
    seen: set[str] = set()
    for item in items:
        if item in seen:
            continue
        seen.add(item)
        out.append(item)
    return out


def load_sources(path: str) -> list[str]:
    sources: list[str] = []
    local = read_local(path)
    if local is not None:
        sources.append(local)

    remote = try_fetch(path)
    if remote is not None:
        sources.append(remote)

    if not sources:
        raise SystemExit(f"failed to load {path} from local snapshot or upstream")

    return dedupe_preserve_order(sources)


# Number of evaluation-stack arguments each syscall consumes.
# Derived from the Neo C# handler method signatures (excluding the implicit
# ApplicationEngine `this` parameter).
PARAM_COUNTS: dict[str, int] = {
    "System.Runtime.Platform": 0,
    "System.Runtime.GetNetwork": 0,
    "System.Runtime.GetAddressVersion": 0,
    "System.Runtime.GetTrigger": 0,
    "System.Runtime.GetTime": 0,
    "System.Runtime.GetScriptContainer": 0,
    "System.Runtime.GetExecutingScriptHash": 0,
    "System.Runtime.GetCallingScriptHash": 0,
    "System.Runtime.GetEntryScriptHash": 0,
    "System.Runtime.LoadScript": 3,
    "System.Runtime.CheckWitness": 1,
    "System.Runtime.GetInvocationCounter": 0,
    "System.Runtime.GetRandom": 0,
    "System.Runtime.Log": 1,
    "System.Runtime.Notify": 2,
    "System.Runtime.GetNotifications": 1,
    "System.Runtime.GasLeft": 0,
    "System.Runtime.BurnGas": 1,
    "System.Runtime.CurrentSigners": 0,
    "System.Contract.Call": 4,
    "System.Contract.CallNative": 1,
    "System.Contract.GetCallFlags": 0,
    "System.Contract.CreateStandardAccount": 1,
    "System.Contract.CreateMultisigAccount": 2,
    "System.Contract.NativeOnPersist": 0,
    "System.Contract.NativePostPersist": 0,
    "System.Storage.GetContext": 0,
    "System.Storage.GetReadOnlyContext": 0,
    "System.Storage.AsReadOnly": 1,
    "System.Storage.Get": 2,
    "System.Storage.Find": 3,
    "System.Storage.Put": 3,
    "System.Storage.Delete": 2,
    "System.Storage.Local.Get": 1,
    "System.Storage.Local.Find": 2,
    "System.Storage.Local.Put": 2,
    "System.Storage.Local.Delete": 1,
    "System.Crypto.CheckSig": 2,
    "System.Crypto.CheckMultisig": 2,
    "System.Iterator.Next": 1,
    "System.Iterator.Value": 1,
}


def parse_registers(text: str) -> Iterable[Syscall]:
    for match in REGISTER_PATTERN.finditer(text):
        name = match.group("name")
        handler = match.group("handler")
        price = " ".join(match.group("price").split())
        flags = " ".join(match.group("flags").split())
        returns_value = not returns_void(name)
        yield Syscall(
            name=name,
            handler=handler,
            price=price,
            call_flags=flags,
            returns_value=returns_value,
            param_count=PARAM_COUNTS.get(name, 0),
        )


def collect_syscalls() -> list[Syscall]:
    entries: dict[str, Syscall] = {}
    for file in FILES:
        for text in load_sources(file):
            for syscall in parse_registers(text):
                entries[syscall.name] = syscall
    return sorted(entries.values(), key=lambda s: (s.hash_value, s.name))


RUST_TEMPLATE = """// This file is @generated by tools/scrape_syscalls.py. Do not edit manually.

pub struct SyscallInfo {{
    pub hash: u32,
    pub name: &'static str,
    pub handler: &'static str,
    pub price: &'static str,
    pub call_flags: &'static str,
    pub returns_value: bool,
    /// Number of evaluation-stack arguments consumed by this syscall.
    pub param_count: u8,
}}

pub const SYSCALLS: &[SyscallInfo] = &[
{entries}
];
"""


def render_entry(syscall: Syscall) -> str:
    return (
        f"    SyscallInfo {{ hash: 0x{syscall.hash_value:08X}, "
        f"name: \"{syscall.name}\", "
        f"handler: \"{syscall.handler}\", "
        f"price: \"{syscall.price}\", "
        f"call_flags: \"{syscall.call_flags}\", "
        f"returns_value: {str(syscall.returns_value).lower()}, "
        f"param_count: {syscall.param_count} }},"
    )


def returns_void(name: str) -> bool:
    return name in {
        "System.Runtime.Notify",
        "System.Runtime.Log",
        "System.Runtime.BurnGas",
        "System.Storage.Put",
        "System.Storage.Delete",
        "System.Storage.Local.Put",
        "System.Storage.Local.Delete",
        "System.Contract.NativePostPersist",
        "System.Contract.NativeOnPersist",
    }


def main() -> None:
    syscalls = collect_syscalls()
    entries = "\n".join(render_entry(s) for s in syscalls)
    OUTPUT.write_text(RUST_TEMPLATE.format(entries=entries))
    meta = [s.as_dict() for s in syscalls]
    data_dir = REPO_ROOT / "tools" / "data"
    data_dir.mkdir(parents=True, exist_ok=True)
    (data_dir / "syscalls.json").write_text(json.dumps(meta, indent=2))
    print(f"wrote {OUTPUT} ({len(syscalls)} entries)")


if __name__ == "__main__":
    main()
