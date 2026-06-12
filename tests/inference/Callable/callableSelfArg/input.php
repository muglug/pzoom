<?php
class C extends B {}

$b = new B();
$c = new C();

$b->func2(function(B $x): void {});
$c->func2(function(B $x): void {});

class A {}

class B extends A {
    /**
     * @param callable(self) $f
     */
    function func2(callable $f): void {
        $f($this);
    }
}
