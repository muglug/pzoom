<?php
/**
 * @psalm-assert array $data
 * @param mixed $data
 */
function isArray($data): void {}

/**
 * @param iterable<string> $arr
 * @return array<string>
 */
function foo(iterable $arr) : array {
    isArray($arr);
    return $arr;
}
