<?php
/**
 * @param array{bar?: int, foo: int|string} $arr
 * @return array{bar: int, foo: string}|null
 */
function foo(array $arr) : ?array {
    if (!isset($arr["bar"])) {
        return null;
    }

    if (is_int($arr["foo"])) {
        return null;
    }

    return $arr;
}