<?php
class A {
    private function foo() : void {}
}

/** @psalm-override-method-visibility */
interface I {}

function takeI(I $i) : void {
    if ($i instanceof A) {
        $i->foo();
    }
}

function takeA(A $a) : void {
    if ($a instanceof I) {
        $a->foo();
    }
}
