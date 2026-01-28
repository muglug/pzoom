<?php
abstract class A {
    /** @var string */
    public $foo;

    public function __construct(int $bar) {
        $this->foo = (string)$bar;
    }
}

class B extends A {}

class E extends \Exception{}
