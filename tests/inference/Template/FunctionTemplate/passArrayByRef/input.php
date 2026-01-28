<?php
function acceptsStdClass(stdClass $_p): void {}

$q = [new stdClass];
acceptsStdClass(fNoRef($q));
acceptsStdClass(fRef($q));
acceptsStdClass(fNoRef($q));

/**
 * @template TKey as array-key
 * @template TValue
 *
 * @param array<TKey, TValue> $_arr
 * @return null|TValue
 * @psalm-ignore-nullable-return
 */
function fRef(array &$_arr) {
    return array_shift($_arr);
}

/**
 * @template TKey as array-key
 * @template TValue
 *
 * @param array<TKey, TValue> $_arr
 * @return null|TValue
 * @psalm-ignore-nullable-return
 */
function fNoRef(array $_arr) {
    return array_shift($_arr);
}