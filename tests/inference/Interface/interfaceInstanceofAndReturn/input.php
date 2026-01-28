<?php
interface A {}
interface B {}

class C implements A, B {}

function foo(A $i): B {
    if ($i instanceof B) {
        return $i;
    }
    throw new \Exception("bad");
}

foo(new C);
