<?php
class A {
    const C_1 = 1;
    const C_2 = 2;
    const C_3 = 3;
    const D_4 = 4;

    /**
     * @param self::C_* $i
     */
    public static function foo(int $i) : void {}
}

A::foo(A::D_4);
