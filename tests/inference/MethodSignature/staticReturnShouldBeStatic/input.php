<?php
class A {
    /** @return static */
    public static function foo() {
        return new static();
    }

    final public function __construct() {}
}

class B extends A {
    public static function foo() {
        return new static();
    }
}

$b = B::foo();
