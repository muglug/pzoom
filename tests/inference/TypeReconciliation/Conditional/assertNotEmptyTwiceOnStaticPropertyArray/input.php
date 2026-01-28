<?php
class A {
    private static array $c = [];

    public static function bar(string $s, string $t): void {
        if (empty(self::$c[$s]) && empty(self::$c[$t])) {}
    }
}