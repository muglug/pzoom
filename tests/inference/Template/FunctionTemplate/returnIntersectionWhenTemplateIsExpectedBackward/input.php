<?php
interface Baz {}

/**
 * @template T as object
 * @param T $t
 * @return Baz&T
 */
function returnsTemplatedIntersection(object $t) {
    return $t;
}
