<?php
/**
 * @template T
 */
class Container {
    /** @var T */
    private $t;

    /** @param T $t */
    public function __construct($t) {
        $this->t = $t;
    }

    /** @return static<T> */
    public function getAnother() {
        return clone $this;
    }
}

/**
 * @template TT
 *
 * @extends Container<TT>
 */
class MyContainer extends Container {}

$a = (new MyContainer("hello"))->getAnother();
