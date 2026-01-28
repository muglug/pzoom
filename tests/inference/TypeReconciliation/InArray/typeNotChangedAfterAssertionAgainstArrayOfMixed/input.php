<?php
/**
 * @param int|null $x
 * @param list<mixed> $y
 * @return int|null
 */
function assertInArray($x, $y) {
    if (!in_array($x, $y, true)) {
        throw new \Exception();
    }

    return $x;
}