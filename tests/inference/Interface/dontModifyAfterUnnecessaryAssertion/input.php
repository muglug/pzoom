<?php
class A {}
interface I {}

/**
 * @param A&I $a
 * @return A&I
 */
function foo(I $a) {
    /** @psalm-suppress RedundantConditionGivenDocblockType */
    assert($a instanceof A);
    return $a;
}
