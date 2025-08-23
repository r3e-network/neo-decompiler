#!/usr/bin/env python3

import subprocess

def analyze_final_5():
    """Analyze the final 5 failing contracts."""
    
    failing_contracts = [
        "Contract_Assignment",
        "Contract_Delegate", 
        "Contract_NULL",
        "Contract_Returns",
        "Contract_String"
    ]
    
    print("=== Final 5 Contract Analysis ===")
    
    for contract in failing_contracts:
        print(f"\n--- {contract} ---")
        
        # Test decompilation to see specific error
        cmd = [
            "./target/release/neo-decompiler", "decompile",
            f"test_data/neo_artifacts/nef_files/{contract}.nef",
            "--manifest", f"test_data/neo_artifacts/manifests/{contract}.manifest.json",
            "--format", "pseudocode"
        ]
        
        try:
            result = subprocess.run(cmd, capture_output=True, text=True, timeout=10)
            if result.returncode == 0:
                print("  ✅ SUCCESS (unexpected)")
            else:
                error_text = result.stdout + result.stderr
                
                # Analyze error type
                if "Truncated instruction" in error_text:
                    print("  ❌ Truncated instruction parsing")
                elif "Stack underflow" in error_text:
                    print("  ❌ Stack management issue")
                elif "Invalid control" in error_text:
                    print("  ❌ Control flow issue")
                elif "Unknown opcode:" in error_text:
                    import re
                    match = re.search(r'Unknown opcode: (0x[0-9a-fA-F]+)', error_text)
                    if match:
                        print(f"  ❌ Missing opcode: {match.group(1)}")
                else:
                    print(f"  ❌ Other: {error_text[:100]}...")
                    
        except subprocess.TimeoutExpired:
            print("  ⏰ Timeout")
        except Exception as e:
            print(f"  ❌ Exception: {e}")

if __name__ == "__main__":
    analyze_final_5()