<?php
class A {}
class B {}

/**
 * @template T of object
 * @param class-string<T> $className
 * @param array<T> $map
 * @param-out array<T> $map
 * @param int $id
 * @return T
 * @psalm-suppress MixedMethodCall
 */
function get(string $className, array &$map, int $id) {
    if(!array_key_exists($id, $map)) {
        $map[$id] = new $className();
    }
    return $map[$id];
}

/**
 * @param array<A> $mapA
 */
function getA(int $id, array $mapA): A {
    return get(A::class, $mapA, $id);
}

/**
 * @param array<B> $mapB
 */
function getB(int $id, array $mapB): B {
    return get(B::class, $mapB, $id);
}