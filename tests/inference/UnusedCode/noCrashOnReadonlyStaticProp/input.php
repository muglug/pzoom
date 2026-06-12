<?php
/** @psalm-immutable */
final class C { public int $val = 2; }

final class A {
    private static C $prop;
    public static function f()
    {
        self::$prop->val = 1;
    }
}

