<?php
/**
 * @template T
 */
class Animal {
    /** @var T */
    private $item;

    /**
     * @param T $item
     */
    public function __construct($item) {
        $this->item = $item;
    }

    /**
     * @return T
     */
    public function get() {
        return $this->item;
    }
}

/**
 * @mixin Animal<self>
 */
class Dog {
    public function __construct() {}
}

function getDog(): Dog {
    return (new Dog())->get();
}
