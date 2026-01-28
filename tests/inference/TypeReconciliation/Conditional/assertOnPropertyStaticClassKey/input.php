<?php
abstract class Obj {
    /** @var array<class-string, array<string, int>> */
    private static $arr = [];

    /** @return array<string, int> */
    public static function getArr() : array {
        $arr = self::$arr;
        if (!isset($arr[static::class])) {
            $arr[static::class] = ["hello" => 5];
        }

        return $arr[static::class];
    }
}