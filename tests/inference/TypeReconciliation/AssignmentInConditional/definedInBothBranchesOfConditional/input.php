<?php
class A {
    public function foo() : void {}
}

function getA(): ?A {
    return rand(0, 1) ? new A() : null;
}

function foo(): void {
    $a = null;
    if (($a = getA()) || ($a = getA())) {
        $a->foo();
    }
}