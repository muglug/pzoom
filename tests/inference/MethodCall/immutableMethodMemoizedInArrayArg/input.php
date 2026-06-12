<?php
/** @psalm-immutable */
abstract class Assertion10 {
    public function getAtomicType(): ?string
    {
        return null;
    }
}
/** @param list<string> $arr */
function takesNonEmpty(array $arr): int { return count($arr); }

function f(Assertion10 $assertion): int {
    if ($assertion->getAtomicType()) {
        return takesNonEmpty([$assertion->getAtomicType()]);
    }
    return 0;
}
