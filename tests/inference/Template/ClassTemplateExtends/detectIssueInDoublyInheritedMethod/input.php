<?php
class Foo {}
class FooChild extends Foo {}

/**
 * @template T0
 */
interface A {
    /**
     * @template U
     * @param callable(T0): U $func
     * @return U
     */
    function test(callable $func);
}

/**
 * @template T1
 * @template-extends A<T1>
 */
interface B extends A {}

/**
 * @template T2
 * @template-extends B<T2>
 */
interface C extends B {}

/**
 * @param C<Foo> $c
 */
function second(C $c) : void {
    $f = function (FooChild $foo) : FooChild { return $foo; };
    $c->test($f);
}
