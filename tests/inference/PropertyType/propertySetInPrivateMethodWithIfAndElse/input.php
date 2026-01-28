<?php
class A {
    /** @var int */
    public $a;

    public function __construct() {
        if (rand(0, 1)) {
            $this->foo();
        } else {
            $this->bar();
        }
    }

    private function foo(): void {
        $this->a = 5;
    }

    private function bar(): void {
        $this->a = 5;
    }
}
