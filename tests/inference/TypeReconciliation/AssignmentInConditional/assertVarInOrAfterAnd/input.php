<?php
class A {}
class B extends A {}
class C extends A {}

function takesA(A $a): void {}

function foo(?A $a, ?A $b): void {
    $c = ($a instanceof B && $b instanceof B) || ($a instanceof C && $b instanceof C);
}