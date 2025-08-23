#!/usr/bin/env python3
"""
Script to test the Neo disassembler against all contract artifacts
and generate disassembly outputs.
"""

import os
import subprocess
import json
from pathlib import Path

def run_disassembler(nef_file, output_dir):
    """Run the disassembler on a single contract."""
    try:
        # Create output directory for this contract
        contract_name = nef_file.stem
        contract_output_dir = output_dir / contract_name
        contract_output_dir.mkdir(exist_ok=True)
        
        # Run the disassembler
        cmd = [
            "./target/release/neo-decompiler",
            "disasm",
            str(nef_file),
            "--output", str(contract_output_dir / f"{contract_name}_disasm.txt"),
            "--offsets",
            "--operands",
            "--comments"
        ]
        
        print(f"Running disassembler on {contract_name}...")
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=30)
        
        # Also try info command
        info_cmd = [
            "./target/release/neo-decompiler",
            "info",
            str(nef_file)
        ]
        
        info_result = subprocess.run(info_cmd, capture_output=True, text=True, timeout=10)
        
        # Save the outputs
        with open(contract_output_dir / "disasm_stdout.log", "w") as f:
            f.write(result.stdout)
        
        with open(contract_output_dir / "disasm_stderr.log", "w") as f:
            f.write(result.stderr)
        
        with open(contract_output_dir / "info_output.log", "w") as f:
            f.write(info_result.stdout)
            if info_result.stderr:
                f.write("\n--- STDERR ---\n")
                f.write(info_result.stderr)
        
        # Save the command that was run
        with open(contract_output_dir / "commands.log", "w") as f:
            f.write("Disasm: " + " ".join(cmd) + "\n")
            f.write("Info: " + " ".join(info_cmd) + "\n")
        
        disasm_success = result.returncode == 0
        info_success = info_result.returncode == 0
        
        return {
            "contract": contract_name,
            "disasm_success": disasm_success,
            "info_success": info_success,
            "disasm_return_code": result.returncode,
            "info_return_code": info_result.returncode,
            "disasm_stdout_length": len(result.stdout),
            "info_stdout_length": len(info_result.stdout),
            "output_files": list(contract_output_dir.glob("*"))
        }
        
    except subprocess.TimeoutExpired:
        return {
            "contract": contract_name,
            "disasm_success": False,
            "info_success": False,
            "error": "Timeout"
        }
    except Exception as e:
        return {
            "contract": contract_name,
            "disasm_success": False,
            "info_success": False,
            "error": str(e)
        }

def main():
    """Main function to test all contracts."""
    # Set up paths
    artifacts_dir = Path("test_data/neo_artifacts")
    nef_dir = artifacts_dir / "nef_files"
    output_dir = Path("disassembled_contracts")
    
    # Create output directory
    output_dir.mkdir(exist_ok=True)
    
    # Get all NEF files
    nef_files = list(nef_dir.glob("*.nef"))
    print(f"Found {len(nef_files)} contracts to test")
    
    results = []
    disasm_successful = 0
    info_successful = 0
    failed = 0
    
    for nef_file in sorted(nef_files):
        result = run_disassembler(nef_file, output_dir)
        results.append(result)
        
        if result.get("disasm_success", False):
            disasm_successful += 1
        
        if result.get("info_success", False):
            info_successful += 1
        
        if not result.get("disasm_success", False) and not result.get("info_success", False):
            failed += 1
        
        contract_name = result["contract"]
        disasm_status = "✅" if result.get("disasm_success", False) else "❌"
        info_status = "✅" if result.get("info_success", False) else "❌"
        
        print(f"{contract_name}: Disasm {disasm_status} | Info {info_status}")
    
    # Save test results
    summary = {
        "total_contracts": len(results),
        "disasm_successful": disasm_successful,
        "info_successful": info_successful,
        "both_failed": failed,
        "disasm_success_rate": disasm_successful / len(results) if results else 0,
        "info_success_rate": info_successful / len(results) if results else 0,
        "results": results
    }
    
    with open(output_dir / "test_results.json", "w") as f:
        json.dump(summary, f, indent=2, default=str)
    
    print(f"\n=== SUMMARY ===")
    print(f"Total contracts tested: {len(results)}")
    print(f"Disasm successful: {disasm_successful}")
    print(f"Info successful: {info_successful}")
    print(f"Both failed: {failed}")
    print(f"Disasm success rate: {summary['disasm_success_rate']:.2%}")
    print(f"Info success rate: {summary['info_success_rate']:.2%}")
    print(f"Results saved to: {output_dir}/test_results.json")

if __name__ == "__main__":
    main()