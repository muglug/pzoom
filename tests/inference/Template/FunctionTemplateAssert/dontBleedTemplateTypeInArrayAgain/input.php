<?php
/**
 * @psalm-template T
 * @psalm-param array<T> $array
 * @psalm-assert array<string, T> $array
 */
function isMap(array $array) : void {}

/**
 * @param array<string> $arr
 */
function bar(array $arr): void {
    isMap($arr);
    /** @psalm-trace $arr */
    $arr;
}
