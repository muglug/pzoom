<?php
/**
 * @psalm-pure
 * @return 2
 */
function example(callable $x) : int {
    assert(is_array($x));
    return count($x);
}
