<?php
/**
 * @template I as object
 */
class Foo {
    /** @var I */
    protected $collection;

    /** @param I $collection */
    public function __construct($collection) {
        $this->collection = $collection;
    }
}

/**
 * @template I2 as object
 *
 * @extends Foo<I2>
 */
class FooChild extends Foo {
    /** @return I2 */
    public function getCollection() {
        return $this->collection;
    }
}