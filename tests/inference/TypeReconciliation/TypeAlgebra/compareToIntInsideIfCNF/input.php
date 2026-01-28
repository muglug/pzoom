<?php
function baz(?int $foo): void {
    if (
        (!$foo || $foo !== 5) && ($foo || rand(0,1)) && ($foo !== 5 || rand(0, 1))
    ) {
        return;
    }

    if ($foo === null) {}
}
