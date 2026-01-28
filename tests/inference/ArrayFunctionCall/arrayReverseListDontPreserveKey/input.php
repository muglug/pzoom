<?php
/** @return list{0: 1, 1: 1.1, 2: 2, 3: false, 4?: string|true, 5?: true} */
function f(): array {
    return [1, 1.1, 2, false, "", true];
}
/** @return list{0: 0, 1: 1, 2: 2, 3?: 3, 4?: 4} */
function g(): array { return [0, 1, 2]; }

$r = array_reverse(f());
$s = array_reverse(g());
