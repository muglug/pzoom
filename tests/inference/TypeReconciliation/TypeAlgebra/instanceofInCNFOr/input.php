<?php
class A {}
class B extends A {}
class C extends A {}

function takesA(A $a): void {}

function foo(?A $a): void {
    $c = rand(0, 1);
    if (($a instanceof B || $a instanceof C)
        && ($a instanceof B || $c)
    ) {
        takesA($a);
    }
}