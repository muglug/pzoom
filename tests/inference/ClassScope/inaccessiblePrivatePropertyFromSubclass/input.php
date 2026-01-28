<?php
class A {
    /** @var string */
    private $fooFoo = "";
}

class B extends A {
    public function doFoo(): void {
        echo $this->fooFoo;
    }
}
