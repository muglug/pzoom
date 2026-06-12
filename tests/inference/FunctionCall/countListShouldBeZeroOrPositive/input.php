<?php
/**
 * @psalm-pure
 * @param list $x
 * @return positive-int|0
 */
function example($x) : int {
    return count($x);
}
