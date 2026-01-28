<?php
class C {
    public function foo() : void {}
}

class B {
    public ?C $c = null;
}

function updateBackgroundClip(?B $b): void {
    if (!$b || !($a = $b->c)) {
        // do something
    } else {
        /** @psalm-suppress MixedMethodCall */
        $a->foo();
    }
}