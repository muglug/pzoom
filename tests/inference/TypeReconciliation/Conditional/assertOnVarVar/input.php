<?php
abstract class Obj {
    /**
     * @param array<class-string, array<string, int>> $arr
     * @return array<string, int>
     */
    function getArr(array $arr, string $s) : array {
        if (!isset($arr[$s])) {
            $arr[$s] = ["hello" => 5];
        }

        return $arr[$s];
    }
}