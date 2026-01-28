<?php
interface A {}
interface B {}

class C implements A, B {}

function takesB(B $b): void {}

function foo(A $i): A {
    if ($i instanceof B) {
        takesB($i);
        return $i;
    }
    return $i;
}

foo(new C);
