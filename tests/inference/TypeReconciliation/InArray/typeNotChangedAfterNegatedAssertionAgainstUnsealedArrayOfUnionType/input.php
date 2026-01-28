<?php
/**
 * @param int|null $x
 * @param non-empty-list<int|null> $y
 * @return int
 */
function assertInArray($x, $y) {
    if (!in_array($x, $y, true)) {
        return $x;
    }
    throw new \Exception();
}
                
