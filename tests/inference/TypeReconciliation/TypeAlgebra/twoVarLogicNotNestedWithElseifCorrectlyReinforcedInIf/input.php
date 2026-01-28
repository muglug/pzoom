<?php
class A {}
class B extends A {}

function foo(?A $a, ?A $b): A {
    if ($a) {
        $a = new B;
    } elseif ($b) {
        // do nothing
    } else {
        return new A;
    }

    if (!$a) return $b;
    return $a;
}