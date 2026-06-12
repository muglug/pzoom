<?php
function f(int $count, int $unique): void {
    $avg = $unique > 0 ? $count / $unique : 0;
    if ($count > 100
        && $avg > 1.1
    ) {
        echo "big";
    }
}
