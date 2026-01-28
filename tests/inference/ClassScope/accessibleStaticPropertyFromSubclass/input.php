<?php
class A {
    /** @var string */
    protected static $fooFoo = "";

    public function barBar(): void {
        echo self::$fooFoo;
    }
}

class B extends A {
    public function doFoo(): void {
        echo A::$fooFoo;
    }
}
