<?php
abstract class A {
    /** @var string */
    public $foo;

    public function __construct() {
        $this->foo = "";
    }
}

class B extends A {
    public function __construct() {
        parent::__construct();
    }
}
