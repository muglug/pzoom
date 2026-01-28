<?php

class A {}
class B {}

/**
 * @template T as A|B
 *
 * @param class-string<T> $class
 * @return class-string<T>
 */
function test(string $class): string {
    return $class;
}

$r = test(A::class);
