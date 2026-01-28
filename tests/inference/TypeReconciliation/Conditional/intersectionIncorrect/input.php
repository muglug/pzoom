<?php
interface I {
    public function bat(): void;
}

interface C {}

/** @param I&C $a */
function takesIandC($a): void {}

class A {
    public function foo(): void {
        if ($this instanceof I) {
            takesIandC($this);
        }
    }
}
