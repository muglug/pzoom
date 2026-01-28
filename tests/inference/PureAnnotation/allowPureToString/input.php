<?php
class A {
    /** @psalm-pure */
    public function __toString() {
        return "bar";
    }
}

/**
 * @psalm-pure
 */
function foo(string $s, A $a) : string {
    if ($a == $s) {}
    return $s;
}
