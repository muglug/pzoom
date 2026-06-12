<?php
class A {}

class B extends A {
    /**
     * @param callable():parent $f
     */
    function func3(callable $f): void {}
}

$b = new B();

$b->func3(function() { return new A(); });
