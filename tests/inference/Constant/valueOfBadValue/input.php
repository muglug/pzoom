<?php
class A {
    const C = [
        1 => "a",
        2 => "b",
        3 => "c"
    ];

    /**
     * @param value-of<A::C> $j
     */
    public static function bar(string $j) : void {}
}

A::bar("d");
