<?php
class A {
    public function foo(): bool {
        return (bool) rand(0, 1);
    }
    public function bar(): bool {
        return (bool) rand(0, 1);
    }
}

/** @return A */
function makeA() {
    return new A;
}

$a = makeA();

if ($a === null) {
    exit;
}

if ($a->foo() || $a->bar()) {}