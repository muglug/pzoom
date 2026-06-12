<?php
function foo(int $t) : void {
    if (!$t) {
        foreach ([0, 1, 2] as $a) {
            if (!$t) {
                $t = $a;
            }
        }
    }
}
