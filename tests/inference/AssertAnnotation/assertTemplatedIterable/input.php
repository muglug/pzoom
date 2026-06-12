<?php
class Foo{}

/**
 * @param array<Foo> $foos
 * @return array<Foo>
 */
function foo(array $foos) : array {
    allIsInstanceOf($foos, Foo::class);
    return $foos;
}

/**
 * @template ExpectedType of object
 *
 * @param mixed $value
 * @param class-string<ExpectedType> $class
 * @psalm-assert iterable<ExpectedType> $value
 */
function allIsInstanceOf($value, $class): void {}
