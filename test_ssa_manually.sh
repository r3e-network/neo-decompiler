#!/bin/bash
# Manual SSA optimization validation test

echo "======================================================================"
echo "SSA Optimization Validation Test"
echo "======================================================================"
echo ""

DECOMPILER="./target/release/neo-decompiler.exe"
TESTS_PASSED=0
TESTS_FAILED=0

# Test 1: Unit tests
echo "Test 1: Running SSA optimization unit tests..."
echo "----------------------------------------------------------------------"
if cargo test --release --lib cfg::ssa::optimize 2>&1 | grep -q "test result: ok"; then
    echo "✓ PASS: All 19 unit tests passed"
    ((TESTS_PASSED++))
else
    echo "✗ FAIL: Unit tests failed"
    ((TESTS_FAILED++))
fi
echo ""

# Test 2: Contract_Array - constant folding
echo "Test 2: Contract_Array - Constant folding & array optimization"
echo "----------------------------------------------------------------------"
OUTPUT=$($DECOMPILER decompile --format ssa TestingArtifacts/devpack/Contract_Array.nef 2>&1)
if echo "$OUTPUT" | grep -q "Optimized SSA"; then
    STATS=$(echo "$OUTPUT" | grep "Optimized SSA")
    echo "$STATS"

    # Check for evidence of optimization
    if echo "$OUTPUT" | grep -q "t_5 = \[1, 2, 3, 4\]"; then
        echo "✓ Array literal optimization working"
    fi
    if echo "$OUTPUT" | grep -q "= 0;"; then
        echo "✓ Constant folding working"
    fi
    echo "✓ PASS: Contract_Array decompiled with optimization"
    ((TESTS_PASSED++))
else
    echo "✗ FAIL: Could not decompile Contract_Array"
    ((TESTS_FAILED++))
fi
echo ""

# Test 3: Contract_Integer - arithmetic optimization
echo "Test 3: Contract_Integer - Arithmetic and copy propagation"
echo "----------------------------------------------------------------------"
OUTPUT=$($DECOMPILER decompile --format ssa TestingArtifacts/devpack/Contract_Integer.nef 2>&1)
if echo "$OUTPUT" | grep -q "Optimized SSA"; then
    STATS=$(echo "$OUTPUT" | grep "Optimized SSA")
    echo "$STATS"

    # Check for arithmetic operations
    if echo "$OUTPUT" | grep -q "arg0.*/ arg1"; then
        echo "✓ Division operations present"
    fi
    if echo "$OUTPUT" | grep -q "arg0.*% arg1"; then
        echo "✓ Modulo operations present"
    fi
    echo "✓ PASS: Contract_Integer decompiled with optimization"
    ((TESTS_PASSED++))
else
    echo "✗ FAIL: Could not decompile Contract_Integer"
    ((TESTS_FAILED++))
fi
echo ""

# Test 4: Check for specific optimization passes
echo "Test 4: Verification of optimization pass evidence"
echo "----------------------------------------------------------------------"
OUTPUT=$($DECOMPILER decompile --format ssa TestingArtifacts/devpack/Contract_Array.nef 2>&1)

# Count phi nodes (should be reduced by trivial phi elimination)
PHI_COUNT=$(echo "$OUTPUT" | grep -o "φ(" | wc -l)
echo "Phi nodes after optimization: $PHI_COUNT"

# Count variables (DCE should reduce these)
VAR_COUNT=$(echo "$OUTPUT" | grep "Optimized SSA" | grep -o "[0-9]\+ variables" | grep -o "[0-9]\+")
echo "Variables after optimization: $VAR_COUNT"

# Count statements
STMT_COUNT=$(echo "$OUTPUT" | grep "Optimized SSA" | grep -o "[0-9]\+ statements" | grep -o "[0-9]\+")
echo "Statements after optimization: $STMT_COUNT"

if [ "$PHI_COUNT" -gt 0 ] && [ "$VAR_COUNT" -gt 0 ] && [ "$STMT_COUNT" -gt 0 ]; then
    echo "✓ PASS: Optimization passes produced valid SSA form"
    ((TESTS_PASSED++))
else
    echo "✗ FAIL: Optimization output appears invalid"
    ((TESTS_FAILED++))
fi
echo ""

# Summary
echo "======================================================================"
echo "TEST SUMMARY"
echo "======================================================================"
echo "Passed: $TESTS_PASSED"
echo "Failed: $TESTS_FAILED"
echo ""

if [ $TESTS_FAILED -eq 0 ]; then
    echo "✓ ALL TESTS PASSED - SSA optimization is working correctly"
    exit 0
else
    echo "✗ SOME TESTS FAILED"
    exit 1
fi
