<?php
class A {
    public function __toString() {
        return "A";
    }
}

/** @param string|object $s */
function foo($s) : void {}

foo(new A);
