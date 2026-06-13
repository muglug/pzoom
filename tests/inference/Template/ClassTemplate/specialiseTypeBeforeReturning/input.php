<?php
class Base {}
class Derived extends Base {}

/**
 * @template T of Base
 */
class Foo {
    /**
     * @param T $t
     */
    public function __construct ($t) {}
}

/**
 * @return Foo<Base>
 */
function returnFooBase() {
    $f = new Foo(new Derived());
    takesFooDerived($f);
    return $f;
}

/**
 * @param Foo<Derived> $foo
 */
function takesFooDerived($foo): void {}
