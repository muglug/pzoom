<?php
/**
 * @param list<int> $l
 * @return list<int>
 */
function takesList(array $l) {
    if (count($l) !== 2) {
        throw new \Exception("bad");
    }

    $l[1] = $l[1] + 1;

    return $l;
}
