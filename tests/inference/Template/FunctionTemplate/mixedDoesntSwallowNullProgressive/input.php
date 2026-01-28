<?php
/**
 * @template E
 * @param E $e
 * @param mixed $d
 * @return ?E
 */
function reduce_values($e, $d)
{
    if (rand(0, 1)) {
        $d = $e;
    }

    if (rand(0, 1)) {
        /** @psalm-suppress MixedReturnStatement */
        return $d;
    }

    return null;
}
