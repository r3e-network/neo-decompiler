#!/usr/bin/env python3
"""
Script to test the Neo decompiler against all contract artifacts
and generate decompiled outputs.
"""

import os
import subprocess
import json
from pathlib import Path

def run_decompiler(nef_file, manifest_file, output_dir):
    """Run the decompiler on a single contract."""
    try:
        # Create output directory for this contract
        contract_name = nef_file.stem
        contract_output_dir = output_dir / contract_name
        contract_output_dir.mkdir(exist_ok=True)
        
        # Run the decompiler
        cmd = [
            "./target/release/neo-decompiler",
            "decompile",
            str(nef_file),
            "--manifest", str(manifest_file),
            "--output", str(contract_output_dir / f"{contract_name}_decompiled.txt"),
            "--format", "pseudocode",
            "--type-inference",
            "--reports",
            "--metrics",
            "--verbose"
        ]
        
        print(f"Running decompiler on {contract_name}...")
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=30)
        
        # Save the output and error logs
        with open(contract_output_dir / "stdout.log", "w") as f:
            f.write(result.stdout)
        
        with open(contract_output_dir / "stderr.log", "w") as f:
            f.write(result.stderr)
        
        # Save the command that was run
        with open(contract_output_dir / "command.log", "w") as f:
            f.write(" ".join(cmd) + "\n")
        
        success = result.returncode == 0
        
        return {
            "contract": contract_name,
            "success": success,
            "return_code": result.returncode,
            "stdout_length": len(result.stdout),
            "stderr_length": len(result.stderr),
            "output_files": list(contract_output_dir.glob("*"))
        }
        
    except subprocess.TimeoutExpired:
        return {
            "contract": contract_name,
            "success": False,
            "return_code": -1,
            "error": "Timeout",
            "timeout": True
        }
    except Exception as e:
        return {
            "contract": contract_name,
            "success": False,
            "error": str(e)
        }

def main():
    """Main function to test all contracts."""
    # Set up paths
    artifacts_dir = Path("test_data/neo_artifacts")
    nef_dir = artifacts_dir / "nef_files"
    manifest_dir = artifacts_dir / "manifests"
    output_dir = Path("decompiled_contracts")
    
    # Create output directory
    output_dir.mkdir(exist_ok=True)
    
    # Get all NEF files
    nef_files = list(nef_dir.glob("*.nef"))
    print(f"Found {len(nef_files)} contracts to test")
    
    results = []
    successful = 0
    failed = 0
    
    for nef_file in sorted(nef_files):
        contract_name = nef_file.stem
        manifest_file = manifest_dir / f"{contract_name}.manifest.json"
        
        if not manifest_file.exists():
            print(f"Warning: No manifest file found for {contract_name}")
            continue
        
        result = run_decompiler(nef_file, manifest_file, output_dir)
        results.append(result)
        
        if result["success"]:
            successful += 1
            print(f"✅ {contract_name} - SUCCESS")
        else:
            failed += 1
            error_msg = result.get('error', f'Return code {result.get("return_code", "unknown")}')
            print(f"❌ {contract_name} - FAILED: {error_msg}")
    
    # Save test results
    summary = {
        "total_contracts": len(results),
        "successful": successful,
        "failed": failed,
        "success_rate": successful / len(results) if results else 0,
        "results": results
    }
    
    with open(output_dir / "test_results.json", "w") as f:
        json.dump(summary, f, indent=2, default=str)
    
    print(f"\n=== SUMMARY ===")
    print(f"Total contracts tested: {len(results)}")
    print(f"Successful: {successful}")
    print(f"Failed: {failed}")
    print(f"Success rate: {summary['success_rate']:.2%}")
    print(f"Results saved to: {output_dir}/test_results.json")

if __name__ == "__main__":
    main()