<?php
/**
 * @template T as object
 * @template S as object
 * @param array<T> $a
 * @param interface-string<S> $type
 * @return array<T&S>
 */
function filter(array $a, string $type): array {
    $result = [];
    foreach ($a as $item) {
        if (is_a($item, $type)) {
            $result[] = $item;
        }
    }
    return $result;
}

interface A {}
interface B {}

/** @var array<A> */
$x = [];
$y = filter($x, B::class);