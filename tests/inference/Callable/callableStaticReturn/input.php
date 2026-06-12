<?php
class A {}

class B extends A {
    /**
     * @param callable():static $f
     */
    function func1(callable $f): void {}
}

final class C extends B {}

$c = new C();

$c->func1(function(): C { return new C(); });
