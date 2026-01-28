<?php
/**
 * @param Collection<Cat> $d
 */
function bar(Dog $c, Collection $d): Dog|Cat {
    $animal = foo($c, $d);
    if ($animal instanceof Dog) {}
    if ($animal instanceof Cat) {}
    return $animal;
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
 * @param T $c
 * @param Collection<T> $d
 * @return T
 */
function foo(object $c, Collection $d): object {
    return rand(0, 1) ? $c : $d->get();
}