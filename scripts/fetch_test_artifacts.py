#!/usr/bin/env python3
"""
Script to fetch Neo N3 contract artifacts from neo-devpack-dotnet repository
and extract NEF files and manifests for decompiler testing.
"""

import os
import re
import json
import base64
import requests
from pathlib import Path

# Base URL for the GitHub raw content
BASE_URL = "https://raw.githubusercontent.com/neo-project/neo-devpack-dotnet/master/tests/Neo.Compiler.CSharp.UnitTests/TestingArtifacts"

# Contract names to download (subset for initial testing)
TEST_CONTRACTS = [
    "Contract1",
    "Contract_Array", 
    "Contract_Assignment",
    "Contract_Assert",
    "Contract_Abort",
    "Contract_ABIAttributes",
    "Contract_ABISafe",
    "Contract_BigInteger",
    "Contract_Concat",
    "Contract_Delegate",
    "Contract_GoTo",
    "Contract_Hash",
    "Contract_Interface",
    "Contract_Json",
    "Contract_Lambda",
    "Contract_List",
    "Contract_Map",
    "Contract_Multiple",
    "Contract_NULL",
    "Contract_Params",
    "Contract_PostfixUnary",
    "Contract_Returns",
    "Contract_Runtime",
    "Contract_StaticVar",
    "Contract_String",
    "Contract_Switch",
    "Contract_Throw",
    "Contract_TryCatch",
    "Contract_Types",
]

def extract_nef_and_manifest(content):
    """Extract NEF and Manifest data from C# artifact file."""
    nef_data = None
    manifest_data = None
    
    # Extract NEF data
    nef_start = content.find('Convert.FromBase64String(@"')
    if nef_start != -1:
        nef_start += len('Convert.FromBase64String(@"')
        nef_end = content.find('")', nef_start)
        if nef_end != -1:
            try:
                nef_base64 = content[nef_start:nef_end]
                nef_data = base64.b64decode(nef_base64)
            except Exception as e:
                print(f"Error decoding NEF: {e}")
    
    # Extract Manifest data
    manifest_start = content.find('ContractManifest.Parse(@"')
    if manifest_start != -1:
        manifest_start += len('ContractManifest.Parse(@"')
        manifest_end = content.find('");', manifest_start)
        if manifest_end != -1:
            try:
                manifest_raw = content[manifest_start:manifest_end]
                manifest_json = manifest_raw.replace('""', '"')
                manifest_data = json.loads(manifest_json)
            except Exception as e:
                print(f"Error parsing manifest: {e}")
    
    return nef_data, manifest_data

def download_contract(contract_name):
    """Download and process a single contract artifact."""
    url = f"{BASE_URL}/{contract_name}.cs"
    print(f"Downloading {contract_name}...")
    
    try:
        response = requests.get(url)
        response.raise_for_status()
        
        nef_data, manifest_data = extract_nef_and_manifest(response.text)
        
        if nef_data and manifest_data:
            return nef_data, manifest_data, response.text
        else:
            print(f"Warning: Could not extract NEF or Manifest from {contract_name}")
            return None, None, response.text
            
    except Exception as e:
        print(f"Error downloading {contract_name}: {e}")
        return None, None, None

def main():
    """Main function to download all test artifacts."""
    # Create directories
    test_data_dir = Path("test_data/neo_artifacts")
    test_data_dir.mkdir(parents=True, exist_ok=True)
    
    nef_dir = test_data_dir / "nef_files"
    manifest_dir = test_data_dir / "manifests" 
    source_dir = test_data_dir / "sources"
    
    nef_dir.mkdir(exist_ok=True)
    manifest_dir.mkdir(exist_ok=True)
    source_dir.mkdir(exist_ok=True)
    
    successful_downloads = 0
    
    for contract_name in TEST_CONTRACTS:
        nef_data, manifest_data, source_code = download_contract(contract_name)
        
        if nef_data and manifest_data:
            # Write NEF file
            nef_path = nef_dir / f"{contract_name}.nef"
            with open(nef_path, "wb") as f:
                f.write(nef_data)
            
            # Write manifest file
            manifest_path = manifest_dir / f"{contract_name}.manifest.json"
            with open(manifest_path, "w") as f:
                json.dump(manifest_data, f, indent=2)
            
            successful_downloads += 1
        
        if source_code:
            # Write source file
            source_path = source_dir / f"{contract_name}.cs"
            with open(source_path, "w") as f:
                f.write(source_code)
    
    print(f"\nDownload complete! Successfully processed {successful_downloads}/{len(TEST_CONTRACTS)} contracts.")
    print(f"Files saved to: {test_data_dir.absolute()}")
    
    # Create index file
    index_path = test_data_dir / "index.json"
    index_data = {
        "description": "Neo N3 contract artifacts for decompiler testing",
        "source": "https://github.com/neo-project/neo-devpack-dotnet",
        "contracts": TEST_CONTRACTS,
        "successful_downloads": successful_downloads,
        "directories": {
            "nef_files": "Compiled NEF bytecode files",
            "manifests": "Contract manifest JSON files", 
            "sources": "Original C# source files"
        }
    }
    
    with open(index_path, "w") as f:
        json.dump(index_data, f, indent=2)

if __name__ == "__main__":
    main()