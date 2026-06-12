<?php
class C extends B {}

$b = new B();
$c = new C();

$b->func1(function(B $x): void {});
$c->func1(function(C $x): void {});

class A {}

class B extends A {
    /**
     * @param callable(static) $f
     */
    function func1(callable $f): void {
        $f($this);
    }
}
