<?php
class A {
    public function test() : bool { return true; }
}

function getMaybeA() : ?A { return rand(0, 1) ? new A : null; }

function foo() : void {
    if (rand(0, 10) && ($a = getMaybeA()) && !$a->test()) {
        return;
    }

    echo isset($a);
}