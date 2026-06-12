<?php
class A {
    /** @var bool */
    private $foo;

    public function __construct() {
        unset($this->foo);
    }
}
