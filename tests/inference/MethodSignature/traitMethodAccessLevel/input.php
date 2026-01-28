<?php
class A {}
class B extends A {}

trait T1 {
    abstract protected static function test(A $x) : void;
}

class C1 {
    use T1;

    private static function test(B $x) : void {}
}
