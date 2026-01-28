<?php
class A {
    /** @var string */
    private static $fooFoo;
}

class B extends A {
    public function doFoo(): void {
        echo A::$fooFoo;
    }
}
