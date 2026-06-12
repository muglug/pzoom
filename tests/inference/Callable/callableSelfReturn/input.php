<?php
class A {}

class B extends A {
    /**
     * @param callable():self $f
     */
    function func2(callable $f): void {}
}

final class C extends B {}

$b = new B();
$c = new C();

$b->func2(function() { return new B(); });
$c->func2(function() { return new C(); });
