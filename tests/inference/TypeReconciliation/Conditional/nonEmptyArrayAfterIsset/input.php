<?php
/**
 * @param array<string, int> $arr
 * @return array<string, int>
 */
function foo(array $arr) : array {
    if (isset($arr["a"])) {
        return $arr;
    }

    return ["b" => 1];
}
