<?php
class C {
    public function foo() : void {}
}

/**
 * @psalm-template T
 * @psalm-param T $t
 * @psalm-param callable(?T):void $callable
 * @return T
 */
function makeConcrete($t, callable $callable) {
    $callable(rand(0, 1) ? $t : null);
    return $t;
}

$c = makeConcrete(new C(), function (?C $c) : void {});