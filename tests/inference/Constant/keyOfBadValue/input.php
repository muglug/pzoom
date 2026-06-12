<?php
class A {
    const C = [
        1 => "a",
        2 => "b",
        3 => "c"
    ];

    /**
     * @param key-of<A::C> $i
     */
    public static function foo(int $i) : void {}
}

A::foo(4);
