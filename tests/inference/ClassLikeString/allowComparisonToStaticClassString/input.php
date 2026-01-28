<?php
class A {
    const CLASSES = ["foobar" => B::class];

    function foo(): bool {
        return self::CLASSES["foobar"] === static::class;
    }
}

class B extends A {}
