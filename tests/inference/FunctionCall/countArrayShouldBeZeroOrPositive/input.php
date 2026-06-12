<?php
/**
 * @psalm-pure
 * @param array $x
 * @return positive-int|0
 */
function example($x) : int {
    return count($x);
}
