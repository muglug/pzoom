<?php
class A {
    /** @var ?bool */
    private $foo;

    public function __construct() {
        echo $this->foo;
    }
}
