<?php
abstract class A {
    /** @var string */
    public $foo;

    private function __construct() {
        $this->foo = "hello";
    }
}

class B extends A {
    public function __construct() {}
}
