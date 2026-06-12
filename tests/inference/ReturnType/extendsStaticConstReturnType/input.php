<?php
class A {
    /** @var int */
    private const FOO = 1;

    /** @return static::FOO */
    public function getFoo() {
        return self::FOO;
    }
}

class B extends A {
    /** @var int */
    private const FOO = 2;

    public function getFoo() {
        return self::FOO;
    }
}
