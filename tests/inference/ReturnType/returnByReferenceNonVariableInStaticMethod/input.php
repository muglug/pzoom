<?php
class Foo {
    public static function &foo(string $x): string {
        return $x . "bar";
    }
}
