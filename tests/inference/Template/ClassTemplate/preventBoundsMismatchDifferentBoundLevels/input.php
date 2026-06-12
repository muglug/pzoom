<?php
/**
 * @param Collection<Dog> $c
 */
function bar(Collection $c): void {
    foo($c, new Cat());
}

/** @template T of object */
interface Collection {
    /** @param T $item */
    public function add(object $item): void;
}

class Cat {}
class Dog {}

/**
 * @template T of object
 * @param Collection<T> $c
 * @param T $d
 */
function foo(Collection $c, object $d): void {
    $c->add($d);
}
