<?php
class A {}
class B extends A {}
class C extends A {}

function takesA(A $a): void {}

function foo(?A $a): void {
    if ($a instanceof B
        || ($a instanceof C && rand(0, 1))
    ) {
        takesA($a);
    }
}