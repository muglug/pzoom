<?php
class Dog {}

/**
 * @template T
 */
class Collection {
    /** @var array<T> */
    protected $arr = [];

    /**
      * @param array<T> $arr
      */
    public function __construct(array $arr) {
        $this->arr = $arr;
    }
}

/**
 * @template T
 * @template V
 * @extends Collection<V>
 */
class CollectionChild extends Collection {
}

$dogs = new CollectionChild([new Dog(), new Dog()]);