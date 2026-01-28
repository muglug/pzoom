<?php
namespace FooFoo;

/**
 * @template T
 * @param T $x
 * @return T
 */
function foo($x) {
    return $x;
}

function bar(string $a): void { }

bar(foo("string"));