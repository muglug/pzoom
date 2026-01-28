<?php
class A {
    public function foo() : void {}
}

function getA(): ?A {
    return rand(0, 1) ? new A() : null;
}

function foo(bool $b): void {
    $a = null;
    if (!$b || !($a = getA())) {
        return;
    }
    $a->foo();
}