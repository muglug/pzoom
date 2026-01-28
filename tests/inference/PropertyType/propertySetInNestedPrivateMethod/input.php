<?php
class A {
    /** @var int */
    public $a;

    public function __construct() {
        $this->foo();
    }

    private function foo(): void {
        $this->bar();
    }

    private function bar(): void {
        $this->a = 5;
    }
}
