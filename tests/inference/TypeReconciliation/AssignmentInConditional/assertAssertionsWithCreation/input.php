<?php
class A {}
class B extends A {}
class C extends A {}

function getA(A $a): ?A {
    return rand(0, 1) ? $a : null;
}

function foo(?A $a, ?A $c): void {
    $c = $a && ($b = getA($a)) && $c ? 1 : 0;
}