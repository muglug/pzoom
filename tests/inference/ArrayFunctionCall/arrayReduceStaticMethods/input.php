<?php
$arr = [2, 3, 4, 5];

class C {
    public static function multiply (int $carry, int $item) : int {
        return $carry * $item;
    }

    public static function multiplySelf(array $arr): int {
        return array_reduce($arr, [self::class, "multiply"], 1);
    }

    public static function multiplyStatic(array $arr): int {
        return array_reduce($arr, [static::class, "multiply"], 1);
    }
}

$self_call_result = C::multiplySelf($arr);
$static_call_result = C::multiplyStatic($arr);
