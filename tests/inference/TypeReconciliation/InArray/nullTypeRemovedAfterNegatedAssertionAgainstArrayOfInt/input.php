<?php
/**
 * @param int|null $x
 * @return int
 */
function assertInArray($x) {
    if (!in_array($x, range(0, 5), true)) {
        throw new \Exception();
    }

    return $x;
}