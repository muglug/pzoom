<?php
class Test {
    public static function length(string $param): int {
        return strlen($param);
    }
}
$method_name = "length";
$closure = Test::$method_name(...);
$length = $closure("test");
