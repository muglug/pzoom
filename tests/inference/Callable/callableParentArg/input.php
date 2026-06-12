<?php
class C extends B {}

$b = new B();
$c = new C();

$b->func3(function(A $x): void {});
$c->func3(function(A $x): void {});

class A {}

class B extends A {
    /**
     * @param callable(parent) $f
     */
    function func3(callable $f): void {
        $f($this);
    }
}
