<?php
namespace Bar;

class A {
    public function foo() : bool {
        return (bool) rand(0, 1);
    }
    public function bar() : bool {
        return (bool) rand(0, 1);
    }
}

/**
 * Asserts that a condition is false.
 *
 * @param bool   $condition
 * @param string $message
 *
 * @psalm-assert false $actual
 */
function assertFalse($condition, $message = "") : void {}

function takesA(A $a) : void {
    assertFalse($a->foo());
    assertFalse($a->bar());
}
