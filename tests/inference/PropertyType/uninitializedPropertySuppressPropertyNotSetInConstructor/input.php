<?php
class A {
    /** @var string */
    public $foo;

    public function __construct() {
        $this->setFoo(); // public method that circumvents checks
        echo strlen($this->foo);
    }

    public function setFoo() : void {
        $this->foo = "foo";
    }
}
