<?php
class A {
    /** @var A|null */
    public static $fooFoo;

    public static function getFoo(): A {
        if (!self::$fooFoo) {
            self::$fooFoo = new A();
        }

        return self::$fooFoo;
    }
}
