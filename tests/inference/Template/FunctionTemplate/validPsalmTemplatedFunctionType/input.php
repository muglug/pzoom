<?php
namespace FooFoo;

/**
 * @psalm-template T
 * @psalm-param T $x
 * @psalm-return T
 */
function foo($x) {
    return $x;
}

function bar(string $a): void { }

bar(foo("string"));