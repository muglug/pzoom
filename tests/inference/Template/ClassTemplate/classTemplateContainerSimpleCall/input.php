<?php
class A {}

/**
 * @template T
 */
class Foo {
    /** @var T */
    public $obj;

    /**
     * @param T $obj
     */
    public function __construct($obj) {
        $this->obj = $obj;
    }

    /**
     * @return T
     */
    public function bar() {
        return $this->obj;
    }
}

$afoo = new Foo(new A());
$afoo_bar = $afoo->bar();