<?php
class A {
    public $foo;

    public function __construct() {
        echo strlen($this->foo);
        $this->foo = "foo";
    }
}
