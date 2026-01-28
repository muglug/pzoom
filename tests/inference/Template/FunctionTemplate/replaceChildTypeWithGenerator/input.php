<?php
/**
 * @template TKey as array-key
 * @template TValue
 * @param Traversable<TKey, TValue> $t
 * @return array<TKey, TValue>
 */
function f(Traversable $t): array {
    $ret = [];
    foreach ($t as $k => $v) $ret[$k] = $v;
    return $ret;
}

/** @return Generator<int, stdClass> */
function g():Generator { yield new stdClass; }

takesArrayOfStdClass(f(g()));

/** @param array<stdClass> $p */
function takesArrayOfStdClass(array $p): void {}