<?php
$add_one = function(int $a): int {
    return $a + 1;
};

/**
 * @param  callable(int) : int $c
 */
function bar(callable $c) : int {
    return $c(1);
}

bar($add_one);
