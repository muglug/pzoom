<?php
/** @method void setA() */
class A {
    /** @var string */
    private $a;

    public function __construct() {
        $this->setA();
    }

    public function __call(string $var, array $args) {}
}
