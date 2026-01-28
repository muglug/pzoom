<?php
class A {
    public function __toString() {
        echo "hi";
        return "bar";
    }
}

/**
 * @psalm-pure
 */
function foo(string $s, A $a) : string {
    return $a . $s;
}
