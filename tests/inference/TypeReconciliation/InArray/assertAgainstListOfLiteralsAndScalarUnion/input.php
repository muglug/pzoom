<?php
/**
 * @param string|bool $x
 * @param non-empty-list<"a"|"b"|int> $y
 * @return "a"|"b"
 */
function assertInArray($x, $y) {
    if (in_array($x, $y, true)) {
        return $x;
    }

    throw new Exception();
}