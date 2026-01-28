<?php
class Foo {
    /** @var int */
    public $id = 0;
}

/**
 * @template T as Foo
 */
class Container {
    /**
     * @var T
     */
    private $obj;

    /**
     * @param T $obj
     */
    public function __construct(Foo $obj) {
        $this->obj = $obj;
    }

    /**
     * @param T $object
     */
    public function bar(Foo $object) : void
    {
        if ($this->obj === $object) {}
    }
}