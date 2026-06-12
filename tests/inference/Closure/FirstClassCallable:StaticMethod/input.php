<?php
class Test {
    public static function length(string $param): int {
        return strlen($param);
    }
}
$closure = Test::length(...);
$length = $closure("test");
