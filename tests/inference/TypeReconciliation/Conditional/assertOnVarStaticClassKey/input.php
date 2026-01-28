<?php
abstract class Obj {
    /**
     * @param array<class-string, array<string, int>> $arr
     * @return array<string, int>
     */
    public static function getArr(array $arr) : array {
        if (!isset($arr[static::class])) {
            $arr[static::class] = ["hello" => 5];
        }

        return $arr[static::class];
    }
}