<?php
class A {
    /** @var string */
    protected $fooFoo = "";
}

class B extends A {
    public function doFoo(): void {
        echo $this->fooFoo;
    }
}
