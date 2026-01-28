<?php
trait T {
    public $foo;
}
class A {
    use T;

    public function __construct() {
        $this->foo = 5;
    }

    public function makeNull(): void {
        $this->foo = null;
    }
}
