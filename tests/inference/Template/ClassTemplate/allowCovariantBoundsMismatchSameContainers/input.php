<?php
/**
 * @param Collection<Dog> $c
 * @param Collection<Cat> $d
 */
function bar(Collection $c, Collection $d): Dog|Cat {
    return foo($c, $d);
}

/** @template-covariant T of object */
interface Collection {
    /** @return T */
    public function get(): object;
}

class Cat {}
class Dog {}

/**
 * @template T of object
 * @param Collection<T> $c
 * @param Collection<T> $d
 * @return T
 */
function foo(Collection $c, Collection $d): object {
    return rand(0, 1) ? $c->get() : $d->get();
}