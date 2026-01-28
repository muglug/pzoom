<?php
function foo(bool $a, bool $b, bool $c): void {
    if (($a && $b) || (!$a && $c)) {
        //
    } else {
        if ($c) {}
    }
}
