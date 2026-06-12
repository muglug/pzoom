<?php
enum A {
    const C_1 = 1;
    const C_2 = 2;
    const C_3 = 3;

    /**
     * @param self::C_* $i
     */
    public static function foo(int $i) : void {}
}

A::foo(A::C_1);
A::foo(A::C_2);
A::foo(A::C_3);
