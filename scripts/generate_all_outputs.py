#!/usr/bin/env python3
"""
Generate all possible outputs from the Neo decompiler for successful contracts.
"""

import os
import subprocess
import json
from pathlib import Path

def generate_outputs_for_contract(nef_file, manifest_file, output_base_dir):
    """Generate all output formats for a contract that can be processed."""
    contract_name = nef_file.stem
    contract_dir = output_base_dir / contract_name
    contract_dir.mkdir(exist_ok=True)
    
    results = {}
    
    # Available output formats
    formats = ["pseudocode", "c", "python", "rust", "typescript", "json", "html"]
    
    for fmt in formats:
        try:
            output_file = contract_dir / f"{contract_name}.{fmt}"
            cmd = [
                "./target/release/neo-decompiler",
                "decompile", 
                str(nef_file),
                "--manifest", str(manifest_file),
                "--output", str(output_file),
                "--format", fmt,
                "--type-inference",
                "--reports",
                "--metrics"
            ]
            
            result = subprocess.run(cmd, capture_output=True, text=True, timeout=30)
            
            success = result.returncode == 0
            results[fmt] = {
                "success": success,
                "return_code": result.returncode,
                "file_size": output_file.stat().st_size if output_file.exists() else 0
            }
            
            if success:
                print(f"  ✅ {contract_name}.{fmt}")
            else:
                print(f"  ❌ {contract_name}.{fmt} - Failed")
                # Save error for debugging
                error_file = contract_dir / f"{contract_name}.{fmt}.error"
                with open(error_file, "w") as f:
                    f.write(f"Return code: {result.returncode}\n")
                    f.write(f"Stdout: {result.stdout}\n")
                    f.write(f"Stderr: {result.stderr}\n")
            
        except Exception as e:
            results[fmt] = {
                "success": False,
                "error": str(e)
            }
            print(f"  ❌ {contract_name}.{fmt} - Exception: {e}")
    
    # Also generate disassembly and info
    try:
        disasm_file = contract_dir / f"{contract_name}.disasm"
        cmd = [
            "./target/release/neo-decompiler",
            "disasm",
            str(nef_file),
            "--output", str(disasm_file),
            "--offsets", "--operands", "--comments"
        ]
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=10)
        results["disasm"] = {"success": result.returncode == 0}
        
        if result.returncode == 0:
            print(f"  ✅ {contract_name}.disasm")
        else:
            print(f"  ❌ {contract_name}.disasm - Failed")
            
    except Exception as e:
        results["disasm"] = {"success": False, "error": str(e)}
    
    try:
        info_file = contract_dir / f"{contract_name}.info"
        cmd = [
            "./target/release/neo-decompiler",
            "info",
            str(nef_file)
        ]
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=10)
        if result.returncode == 0:
            with open(info_file, "w") as f:
                f.write(result.stdout)
            results["info"] = {"success": True}
            print(f"  ✅ {contract_name}.info")
        else:
            results["info"] = {"success": False}
            print(f"  ❌ {contract_name}.info - Failed")
            
    except Exception as e:
        results["info"] = {"success": False, "error": str(e)}
    
    return results

def main():
    """Generate outputs for all contracts."""
    artifacts_dir = Path("test_data/neo_artifacts")
    nef_dir = artifacts_dir / "nef_files"
    manifest_dir = artifacts_dir / "manifests"
    output_dir = Path("generated_outputs")
    
    output_dir.mkdir(exist_ok=True)
    
    nef_files = list(nef_dir.glob("*.nef"))
    print(f"Generating outputs for {len(nef_files)} contracts...")
    
    all_results = {}
    total_successful = 0
    
    for nef_file in sorted(nef_files):
        contract_name = nef_file.stem
        manifest_file = manifest_dir / f"{contract_name}.manifest.json"
        
        if not manifest_file.exists():
            print(f"❌ {contract_name} - No manifest file")
            continue
        
        print(f"\n🔄 Processing {contract_name}...")
        results = generate_outputs_for_contract(nef_file, manifest_file, output_dir)
        all_results[contract_name] = results
        
        # Count successful formats
        successful_formats = sum(1 for r in results.values() if r.get("success", False))
        total_successful += successful_formats
        print(f"  📊 {successful_formats}/{len(results)} formats successful")
    
    # Save summary
    summary = {
        "total_contracts": len(nef_files),
        "total_attempts": sum(len(r) for r in all_results.values()),
        "total_successful": total_successful,
        "results": all_results
    }
    
    with open(output_dir / "generation_summary.json", "w") as f:
        json.dump(summary, f, indent=2, default=str)
    
    print(f"\n📈 FINAL SUMMARY")
    print(f"Contracts processed: {len(all_results)}")
    print(f"Total attempts: {summary['total_attempts']}")
    print(f"Total successful: {total_successful}")
    print(f"Overall success rate: {total_successful/summary['total_attempts']:.2%}")

if __name__ == "__main__":
    main()