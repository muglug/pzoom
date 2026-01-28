<?php
class A {
    /** @var string */
    public static $foo = "a";

    public static function barBar(): void
    {
        self::$foo = 5;
    }
}
