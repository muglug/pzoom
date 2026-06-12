<?php
class Foo{
    public function bar() : void {}
}

/**
 * @param mixed $b
 * @psalm-assert int|Foo $b
 */
function assertIntOrFoo($b) : void {
    if (!is_int($b) && !(is_object($b) && $b instanceof Foo)) {
        throw new \Exception("bad");
    }
}

/** @psalm-suppress MixedAssignment */
$a = $GLOBALS["a"];

assertIntOrFoo($a);

if (!is_int($a)) $a->bar();
