<?php
enum A {
    case C_1;
    case C_2;
    case C_3;

    /**
     * @param self::C_* $i
     */
    public static function foo(self $i) : void {}
}

A::foo(A::C_1);
A::foo(A::C_2);
A::foo(A::C_3);
