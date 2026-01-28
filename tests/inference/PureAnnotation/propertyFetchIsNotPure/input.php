<?php
class A {
    public string $foo = "hello";

    /** @psalm-pure */
    public static function getFoo(A $a) : string {
        return $a->foo;
    }
}
