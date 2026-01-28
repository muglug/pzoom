<?php
interface I1 {
    const A = 5;
    const B = "two";
    const C = 3.0;
}

interface I2 extends I1 {
    const D = 5;
    const E = "two";
}

class A implements I2 {
    /** @var int */
    public $foo = I1::A;

    /** @var string */
    public $bar = self::B;

    /** @var float */
    public $bar2 = I2::C;

    /** @var int */
    public $foo2 = I2::D;

    /** @var string */
    public $bar3 = self::E;
}
