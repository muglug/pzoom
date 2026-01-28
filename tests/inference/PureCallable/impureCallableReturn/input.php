<?php
/**
 * @psalm-pure
 * @return pure-callable():int
 */
function foo(): callable {
    return function() {
        echo "bar";
        return 1;
    };
}
