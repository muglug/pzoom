<?php

class ArrayType {}

class TypeResult {
    public function hasType(string $s): bool { return true; }
    public function getArray(): ?ArrayType { return rand(0, 1) ? new ArrayType() : null; }
}

function getMaybe(): ?TypeResult {
    return rand(0, 1) ? new TypeResult() : null;
}

// A variable assigned inside a `&&` chain (and not the left-most operand) must be
// typed for later operands of the chain and for the ternary's true branch — Psalm
// analyzes ternary conditions through IfConditionalAnalyzer, so the nested `&&`
// boils its right-operand assignment into the condition's body context rather than
// discarding it (which would leave `$t`/`$a` mixed and mis-report
// InvalidMethodCall/MixedAssignment).
function test(bool $cond): ?ArrayType {
    return $cond
        && ($t = getMaybe())
        && $t->hasType('array')
        && ($a = $t->getArray())
        ? $a
        : null;
}
