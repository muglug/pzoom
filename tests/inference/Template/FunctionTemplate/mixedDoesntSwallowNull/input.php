<?php
/**
 * @template E
 * @param E $e
 * @param mixed $d
 * @return ?E
 */
function reduce_values($e, $d) {
    if (rand(0, 1)) {
        $c = $e;
    } elseif (rand(0, 1)) {
        /** @psalm-suppress MixedAssignment */
        $c = $d;
    } else {
        $c = null;
    }

    /** @psalm-suppress MixedReturnStatement */
    return $c;
}
