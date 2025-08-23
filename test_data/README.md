# Neo N3 Contract Test Data

This directory contains Neo N3 smart contract artifacts used for testing the decompiler.

## Source

The contract artifacts are sourced from the official Neo .NET DevPack repository:
- Repository: https://github.com/neo-project/neo-devpack-dotnet
- Path: `tests/Neo.Compiler.CSharp.UnitTests/TestingArtifacts`

## Structure

### `neo_artifacts/`

Contains 22 Neo N3 contract testing artifacts:

- **`nef_files/`** - Compiled NEF (Neo Executable Format) bytecode files
- **`manifests/`** - Contract manifest JSON files containing ABI and metadata
- **`sources/`** - Original C# source files from the Neo DevPack test suite

### Contracts Included

1. Contract1 - Basic contract functionality
2. Contract_ABIAttributes - ABI attribute testing
3. Contract_ABISafe - Safe method annotations
4. Contract_Abort - Contract abortion scenarios
5. Contract_Array - Array operations and manipulation
6. Contract_Assert - Assertion testing
7. Contract_Assignment - Variable assignments
8. Contract_BigInteger - Large number operations
9. Contract_Concat - String concatenation
10. Contract_Delegate - Delegate function patterns
11. Contract_GoTo - Jump and control flow
12. Contract_Lambda - Lambda expressions
13. Contract_NULL - Null value handling
14. Contract_Params - Parameter passing
15. Contract_PostfixUnary - Postfix operators
16. Contract_Returns - Return value handling
17. Contract_StaticVar - Static variable usage
18. Contract_String - String operations
19. Contract_Switch - Switch statement logic
20. Contract_Throw - Exception throwing
21. Contract_TryCatch - Exception handling
22. Contract_Types - Type system features

## Usage

These artifacts are used to test and validate the Neo decompiler's ability to:

1. Parse NEF files correctly
2. Extract contract metadata from manifests
3. Disassemble bytecode into readable instructions
4. Analyze control flow and data flow
5. Generate human-readable pseudocode
6. Detect security patterns and vulnerabilities

## Test Results

The decompiler has been tested against all 22 contracts with the following results:

- **NEF Parsing**: 100% success rate (22/22)
- **Info Extraction**: 100% success rate (22/22) 
- **Basic Disassembly**: 100% success rate (22/22)
- **Full Decompilation**: Limited due to CFG construction issues

## Scripts

- `scripts/fetch_test_artifacts.py` - Downloads and extracts artifacts from GitHub
- `scripts/test_disasm.py` - Tests disassembler against all contracts
- `scripts/test_decompiler.py` - Tests full decompiler pipeline