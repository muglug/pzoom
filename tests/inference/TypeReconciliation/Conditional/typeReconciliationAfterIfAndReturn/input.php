<?php
/**
 * @param string|int $a
 * @return string|int
 */
function foo($a) {
    if (is_string($a)) {
        return $a;
    } elseif (is_int($a)) {
        return $a;
    }

    throw new \LogicException("Runtime error");
}