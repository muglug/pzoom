<?php
class A {}
class B extends A {}

function foo(A $a, A $b) : ?B {
    if (($a instanceof B || !$b instanceof B) && $a instanceof B && $b instanceof B) {
        return $a;
    }

    return null;
}