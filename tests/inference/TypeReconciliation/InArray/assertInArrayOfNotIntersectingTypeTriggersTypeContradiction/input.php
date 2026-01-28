<?php
/**
 * @param int $x
 * @param list<string> $y
 * @return int
 */
function assertInArray($x, $y) {
    if (in_array($x, $y, true)) {
        throw new \Exception();
    }

    return $x;
}
