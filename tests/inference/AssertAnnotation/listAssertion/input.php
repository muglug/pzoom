<?php
/**
 * @psalm-assert list $data
 * @param mixed $data
 */
function isList($data): void {}

/**
 * @param array<string> $arr
 * @return list<string>
 */
function foo(array $arr) : array {
    isList($arr);
    return $arr;
}
