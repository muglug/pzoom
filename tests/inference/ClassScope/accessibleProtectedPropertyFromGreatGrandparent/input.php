<?php
class A {
    /** @var string */
    protected $fooFoo = "";
}

class B extends A { }

class C extends B { }

class D extends C {
    public function doFoo(): void {
        echo $this->fooFoo;
    }
}
