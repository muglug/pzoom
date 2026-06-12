<?php
/**
 * @psalm-pure
 * @param array{int, int, string} $x
 * @return 3
 */
function example($x) : int {
    return count($x);
}
