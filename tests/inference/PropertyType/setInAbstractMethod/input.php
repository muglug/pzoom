<?php
interface I {
    public function foo(): void;
}

/** @psalm-suppress PropertyNotSetInConstructor */
abstract class A implements I {
    /** @var string */
    public $bar;

    public function __construct() {
        $this->foo();
    }
}

class B extends A {
    public function foo(): void {
        $this->bar = "hello";
    }
}
