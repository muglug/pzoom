<?php
function foo(bool $a, bool $b, bool $c): void {
    if ((!$a && !$b) || ($a && $b) || ($a && $c)) {
        throw new \Exception();
    }

    if ($a) {}
}
