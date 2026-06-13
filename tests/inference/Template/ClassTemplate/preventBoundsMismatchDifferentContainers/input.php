<?php
/**
 * @param Collection1<Dog> $c
 * @param Collection2<Cat> $d
 */
function bar(Collection1 $c, Collection2 $d): void {
    foo($c, $d);
}

/** @template T of object */
interface Collection1 {
    /** @param T $item */
    public function add(object $item): void;
}

/** @template T of object */
interface Collection2 {
    /** @param T $item */
    public function add(object $item): void;

    /** @return T */
    public function get(): object;
}

class Cat {}
class Dog {}

/**
 * @template T of object
 * @param Collection1<T> $c
 * @param Collection2<T> $d
 */
function foo(Collection1 $c, Collection2 $d): void {
    $c->add($d->get());
}
