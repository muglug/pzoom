<?php
/** @psalm-pure */
function addCumulative(int $left) : int {
    /** @var int */
    static $i = 0;
    $i += $left;
    return $left;
}
