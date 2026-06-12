<?php
/** @param non-empty-list<int|null> $min_bounds */
function f(array $min_bounds): void {
    $min_potential_int = in_array(null, $min_bounds, true) ? null : max($min_bounds);
    if ($min_potential_int === null) {
        echo 'unbounded';
    } else {
        echo $min_potential_int;
    }
}
