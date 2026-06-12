<?php
class A {
    private static string $prev = "";

    public static function getPrevious(string $s): string {
        $prev = self::$prev;
        self::$prev = $s;
        return $prev;
    }
}

A::getPrevious($_GET["a"]);
echo A::getPrevious("foo");
