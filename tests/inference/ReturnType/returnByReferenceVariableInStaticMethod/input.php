<?php
class Foo {
    private static string $foo = "foo";

    public static function &foo(): string {
        return self::$foo;
    }
}
