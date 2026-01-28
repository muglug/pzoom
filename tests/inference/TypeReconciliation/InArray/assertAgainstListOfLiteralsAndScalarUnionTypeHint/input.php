<?php
/**
 * @param non-empty-list<"a"|"b"|int> $y
 * @return "a"|"b"
 */
function assertInArray(string|bool $x, $y) {
    if (in_array($x, $y, true)) {
        return $x;
    }

    throw new Exception();
}