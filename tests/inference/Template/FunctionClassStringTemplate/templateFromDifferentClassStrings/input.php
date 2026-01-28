<?php
/**
 * @psalm-consistent-constructor
 */
class A {}

class B extends A {}
class C extends A {}

/**
 * @template T of A
 * @param class-string<T> $a1
 * @param class-string<T> $a2
 * @return T
 */
function test(string $a1, string $a2) {
    if (rand(0, 1)) return new $a1();

    return new $a2();
}

$b_or_c = test(B::class, C::class);