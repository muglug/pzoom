<?php
/**
 * @param Collection<Dog> $c
 * @param Collection<Cat> $d
 */
function bar(Collection $c, Collection $d): void {
    foo($c, $d);
}

/** @template T of object */
interface Collection {
    /** @param T $item */
    public function add(object $item): void;

    /** @return T */
    public function get(): object;
}

class Cat {}
class Dog {}

/**
 * @template T of object
 * @param Collection<T> $c
 * @param Collection<T> $d
 */
function foo(Collection $c, Collection $d): void {
    $c->add($d->get());
}
