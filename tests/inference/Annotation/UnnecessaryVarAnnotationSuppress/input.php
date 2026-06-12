<?php
/** @psalm-consistent-constructor */
final class Foo{}
/**
 * @param class-string $class
 */
function foo(string $class): Foo {
    if (!is_subclass_of($class, Foo::class)) {
        throw new \LogicException();
    }

    /**
     * @psalm-suppress UnnecessaryVarAnnotation
     * @var Foo $instance
     */
    $instance = new $class();

    return $instance;
}
