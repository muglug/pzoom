<?php
/**
 * @psalm-pure
 * @param non-empty-list $x
 * @return positive-int
 */
function example($x) : int {
    return count($x);
}
