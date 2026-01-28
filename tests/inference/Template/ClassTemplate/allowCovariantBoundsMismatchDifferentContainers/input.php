<?php
/**
 * @param Collection1<Dog> $c
 * @param Collection2<Cat> $d
 */
function bar(Collection1 $c, Collection2 $d): Dog|Cat {
    return foo($c, $d);
}

/** @template-covariant T of object */
interface Collection1 {
    /** @return T */
    public function get(): object;
}

/** @template-covariant T of object */
interface Collection2 {
    /** @return T */
    public function get(): object;
}

class Cat {}
class Dog {}

/**
 * @template T of object
 * @param Collection1<T> $c
 * @param Collection2<T> $d
 * @return T
 */
function foo(Collection1 $c, Collection2 $d): object {
    return rand(0, 1) ? $c->get() : $d->get();
}