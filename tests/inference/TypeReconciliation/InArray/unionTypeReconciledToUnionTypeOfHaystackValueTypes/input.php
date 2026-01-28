<?php
/**
 * @param int|string|bool $x
 * @param non-empty-list<int|string> $y
 * @return int|string
 */
function assertInArray($x, $y) {
    if (in_array($x, $y, true)) {
        return $x;
    }
    throw new \Exception();
}
                