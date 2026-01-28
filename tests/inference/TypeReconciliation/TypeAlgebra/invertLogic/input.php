<?php
class A {}
class B extends A {}

function foo(?A $a) : A {
    if (!$a || !($a instanceof B && rand(0, 1))) {
        throw new Exception();
    }

    return $a;
}