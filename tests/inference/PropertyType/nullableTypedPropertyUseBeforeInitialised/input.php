<?php
class A {
    private ?bool $foo;

    public function __construct() {
        echo $this->foo;
    }
}
