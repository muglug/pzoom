<?php
abstract class A {}
class B extends A {
    public string $value = "";
}

function takesString(string $s): void {}

/**
 * @param A $left
 * @param A $right
 */
function f($left, $right): void {
    /**
     * @var A $left
     * @var A $right
     */

    if (($left instanceof B && strtolower($left->value) === 'gmp')
        || ($right instanceof B && strtolower($right->value) === 'gmp')
    ) {
        takesString($left instanceof B ? $left->value : "");
    }
}
