<?php
/** @return positive-int|0 */
function example(\Countable $x) : int {
    return count($x);
}
