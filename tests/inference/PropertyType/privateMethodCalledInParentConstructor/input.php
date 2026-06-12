<?php
class C extends B {}

abstract class B extends A {
    /** @var string */
    private $b;

    /** @var string */
    protected $c;
}

class A {
    public function __construct() {
        $this->publicMethod();
    }

    public function publicMethod() : void {
        $this->privateMethod();
    }

    private function privateMethod() : void {}
}
