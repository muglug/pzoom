<?php
/**
 * @param int|bool $x
 * @param non-empty-list<int|string> $y
 * @return int
 */
function assertInArray($x, $y) {
    if (in_array($x, $y, true)) {
        throw new Exception();
    }
    return $x;
}
