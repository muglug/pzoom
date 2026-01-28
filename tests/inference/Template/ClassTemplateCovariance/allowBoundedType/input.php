<?php
class Base {}
class Child extends Base {}

/**
 * @template-covariant T
 */
class Foo
{
    /** @param Closure():T $t */
    public function __construct(Closure $t) {}
}

/**
 * @return Foo<Base>
 */
function returnFooBase() : Foo {
    $f = new Foo(function () { return new Child(); });
    return $f;
}