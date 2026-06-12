<?php
class A {
    const C = [
        1 => "a",
        2 => "b",
        3 => "c"
    ];

    /**
     * @var value-of<self::C>
     */
    public $foo = "a";
}
