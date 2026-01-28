<?php
/**
 * @template T as object
 * @template S as object
 * @param T $item
 * @param interface-string<S> $type
 * @return T&S
 */
function filter($item, string $type) {
    if (is_a($item, $type)) {
        return $item;
    };

    throw new \UnexpectedValueException("bad");
}

interface A {}
interface B {}

/** @var A */
$x = null;

$y = filter($x, B::class);
