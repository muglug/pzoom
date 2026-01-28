<?php
/**
 * @param int $x
 * @param list<string> $y
 * @return string
 */
function assertInArray($x, $y) {
    if (in_array($x, $y, true)) {
        return $x;
    }

    throw new \Exception();
}
