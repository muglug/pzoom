<?php
interface Baz {}

/**
 * @template T as object
 * @param T $t
 * @return T&Baz
 */
function returnsTemplatedIntersection(object $t) {
    return $t;
}
