<?php
class A {
    /** @var ?bool */
    private ?bool $foo;

    public function __construct() {
        echo $this->foo;
    }
}
