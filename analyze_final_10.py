#!/usr/bin/env python3
"""
Analyze the final 10 failing contracts to understand exact issues and prioritize fixes.
"""

import subprocess
import re

def analyze_failing_contract(contract_name):
    """Analyze a specific failing contract in detail."""
    print(f"\n=== {contract_name} ===")
    
    # Test each stage to see where it fails
    nef_file = f"test_data/neo_artifacts/nef_files/{contract_name}.nef"
    manifest_file = f"test_data/neo_artifacts/manifests/{contract_name}.manifest.json"
    
    stages = [
        ("INFO", ["./target/release/neo-decompiler", "info", nef_file]),
        ("DISASM", ["./target/release/neo-decompiler", "disasm", nef_file]),
        ("DECOMPILE", ["./target/release/neo-decompiler", "decompile", nef_file, "--manifest", manifest_file, "--format", "pseudocode"])
    ]
    
    failure_stage = None
    error_details = None
    
    for stage_name, cmd in stages:
        try:
            result = subprocess.run(cmd, capture_output=True, text=True, timeout=10)
            if result.returncode == 0:
                print(f"  ✅ {stage_name}: SUCCESS")
            else:
                print(f"  ❌ {stage_name}: FAILED")
                failure_stage = stage_name
                error_details = result.stdout + result.stderr
                
                # Extract specific error type
                if "Unknown opcode:" in error_details:
                    match = re.search(r'Unknown opcode: (0x[0-9a-fA-F]+)', error_details)
                    if match:
                        print(f"     Missing opcode: {match.group(1)}")
                
                elif "Truncated instruction" in error_details:
                    match = re.search(r'Truncated instruction at offset (\d+)', error_details)
                    if match:
                        print(f"     Truncated at offset: {match.group(1)}")
                
                elif "Stack underflow" in error_details:
                    match = re.search(r'Stack underflow when lifting instruction at offset (\d+)', error_details)
                    if match:
                        print(f"     Stack underflow at offset: {match.group(1)}")
                
                elif "Invalid control" in error_details:
                    print(f"     Control flow validation error")
                    
                elif "CFG construction" in error_details:
                    print(f"     CFG construction error")
                
                else:
                    print(f"     Other: {error_details[:100]}...")
                    
                break  # Stop at first failure
                
        except subprocess.TimeoutExpired:
            print(f"  ⏰ {stage_name}: TIMEOUT")
            failure_stage = stage_name
            break
        except Exception as e:
            print(f"  ❌ {stage_name}: EXCEPTION - {e}")
            failure_stage = stage_name
            break
    
    return failure_stage, error_details

def main():
    """Analyze all 10 failing contracts."""
    
    failing_contracts = [
        "Contract_Assignment",
        "Contract_Concat", 
        "Contract_Delegate",
        "Contract_Lambda",
        "Contract_NULL",
        "Contract_PostfixUnary",
        "Contract_String",
        "Contract_Switch", 
        "Contract_TryCatch",
        "Contract_Types"
    ]
    
    print("=== Analyzing Final 10 Failing Contracts for 100% Success ===")
    
    issue_categories = {
        "unknown_opcodes": [],
        "truncated_instructions": [],
        "stack_underflow": [],
        "control_flow": [],
        "cfg_construction": [],
        "other": []
    }
    
    for contract in failing_contracts:
        failure_stage, error_details = analyze_failing_contract(contract)
        
        if error_details:
            if "Unknown opcode:" in error_details:
                issue_categories["unknown_opcodes"].append(contract)
            elif "Truncated instruction" in error_details:
                issue_categories["truncated_instructions"].append(contract)
            elif "Stack underflow" in error_details:
                issue_categories["stack_underflow"].append(contract)
            elif "Invalid control" in error_details:
                issue_categories["control_flow"].append(contract)
            elif "CFG construction" in error_details:
                issue_categories["cfg_construction"].append(contract)
            else:
                issue_categories["other"].append(contract)
    
    print(f"\n=== FAILURE CATEGORIZATION ===")
    for category, contracts in issue_categories.items():
        if contracts:
            print(f"{category}: {len(contracts)} contracts")
            print(f"  {', '.join(contracts)}")
            print()
    
    # Prioritize fixes
    print("=== FIX PRIORITY ===")
    print("1. Unknown opcodes (easiest to fix)")
    print("2. Truncated instructions (bytecode parsing)")
    print("3. Stack underflow (logic fixes)")
    print("4. Control flow issues (complex)")
    print("5. CFG construction (advanced)")

if __name__ == "__main__":
    main()