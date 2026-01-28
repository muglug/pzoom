<?php
class A {
    const FOO = "foo";
    const BAR = "bar";

    /**
     * @param (self::1FOO | self::BAR) $s
     */
    public static function foo(string $s) : void {}
}
