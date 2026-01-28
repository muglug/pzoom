<?php
class A {
    /** @var string */
    protected $fooFoo = "";
}

class B extends A {
}

class C extends A {
    public function fooFoo(): void {
        $b = new B();
        $b->fooFoo = "hello";
    }
}
