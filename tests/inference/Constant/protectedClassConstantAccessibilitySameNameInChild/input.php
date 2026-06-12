<?php
class A {
    /** @var int<1,max> */
    protected const A = 1;

    public static function test(): void {
        echo B::A;
    }
}

class B extends A {
    protected const A = 2;
}

A::test();
