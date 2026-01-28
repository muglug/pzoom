<?php
/**
 * @psalm-consistent-constructor
 */
class A {}

/**
 * @psalm-consistent-constructor
 */
class B {}

/**
 * @template T1 as A
 * @template T2 as B
 * @param class-string<T1>|class-string<T2> $type
 * @return T1|T2
 */
function f(string $type) {
    return new $type();
}

f(A::class);
f(B::class);