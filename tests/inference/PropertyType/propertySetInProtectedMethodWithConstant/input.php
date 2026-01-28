<?php
class A {
    /** @var int */
    public $a;

    public function __construct() {
        $this->foo();
    }

    protected function foo(): void {
        $this->a = 5;
    }
}

class B extends A {
    const HELLO = "HELLO";

    protected function foo() : void {
        $this->a = 6;

        echo self::HELLO;
    }
}
