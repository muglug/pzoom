<?php
class A {
    public function foo() : void {}
}

function getA(): ?A {
    return rand(0, 1) ? new A() : null;
}

function foo(): void {
    if (($a = getA()) || rand(0, 1)) {
        if ($a) {
            $a->foo();
        }
    }
}